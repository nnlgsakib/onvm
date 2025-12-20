use super::DagEngine;
use anyhow::{anyhow, Result};
use libp2p::PeerId;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

impl DagEngine {
    pub async fn submit_execution(
        &self,
        program_id: &crate::types::ProgramId,
        input: &[u8],
    ) -> Result<crate::wasm_runtime::ExecutionOutcome> {
        let object_id = program_id.to_object_id();

        if !self.unified_store.is_complete(&object_id)? {
            tracing::info!(
                "program {} not available locally, attempting to fetch from network",
                program_id
            );

            if let Some(ref distributor) = self.chunk_distributor {
                match distributor
                    .fetch_object(&object_id, crate::network::ProviderKind::Program)
                    .await
                {
                    Ok(data) => {
                        tracing::info!(
                            "successfully fetched program {} from network ({} bytes)",
                            program_id,
                            data.len()
                        );
                    }
                    Err(e) => {
                        return Err(anyhow!(
                            "program not available locally and fetch from network failed: {}",
                            e
                        ));
                    }
                }
            } else {
                return Err(anyhow!(
                    "program not available and chunk distributor not initialized"
                ));
            }

            if !self.unified_store.is_complete(&object_id)? {
                return Err(anyhow!(
                    "program fetch completed but object is still incomplete"
                ));
            }
        }

        let has_state = {
            let versions = self.program_state_versions.read().await;
            versions.contains_key(program_id)
        };

        if !has_state {
            let in_progress = self
                .state_sync_in_progress
                .read()
                .await
                .contains(program_id);

            if !in_progress {
                self.state_sync_in_progress
                    .write()
                    .await
                    .insert(program_id.clone());

                let state_store = Arc::clone(&self.state_store);
                let network = self.network.clone();
                let peers = Arc::clone(&self.peers);
                let versions = Arc::clone(&self.program_state_versions);
                let in_progress = Arc::clone(&self.state_sync_in_progress);
                let pid = program_id.clone();

                tokio::spawn(async move {
                    if let Err(e) = Self::background_state_sync_static(
                        &state_store,
                        &network,
                        &peers,
                        &versions,
                        &pid,
                    )
                    .await
                    {
                        tracing::debug!("background state sync failed for {}: {}", pid, e);
                    }
                    in_progress.write().await.remove(&pid);
                });
            }
        }

        let outcome = self.scheduler.execute(program_id, input).await?;

        if !outcome.state_writes.is_empty() {
            let new_version = {
                let mut versions = self.program_state_versions.write().await;
                let version = versions.entry(program_id.clone()).or_insert(0);
                *version += 1;
                *version
            };

            let sync_msg = crate::network::StateSyncMessage {
                program_id: program_id.clone(),
                state_writes: outcome.state_writes.clone(),
                state_root: outcome.state_root,
                executor_node: self.identity.node_id.clone(),
                timestamp_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
            };

            let msg = crate::network::NetworkMessage::StateSync(sync_msg);
            let _ = self.network.publisher.send(msg);

            tracing::debug!(
                "broadcasted {} state writes for program {} (version {})",
                outcome.state_writes.len(),
                program_id,
                new_version
            );
        }

        Ok(outcome)
    }

    async fn background_state_sync_static(
        state_store: &Arc<crate::storage::StateStore>,
        network: &crate::network::NetworkHandle,
        peers: &Arc<RwLock<HashSet<PeerId>>>,
        versions: &Arc<RwLock<HashMap<crate::types::ProgramId, u64>>>,
        program_id: &crate::types::ProgramId,
    ) -> Result<()> {
        let peer_list: Vec<_> = peers.read().await.iter().copied().collect();

        if peer_list.is_empty() {
            return Err(anyhow!("no peers available"));
        }

        for peer in peer_list.iter().take(3) {
            let req = crate::network::StateRequest {
                program_id: program_id.clone(),
            };

            network.request_transfer(*peer, crate::network::TransferRequest::StateRequest(req));

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            let state_count = state_store.get_all_scoped(&program_id.0)?.len();
            if state_count > 0 {
                versions.write().await.insert(program_id.clone(), 1);
                tracing::info!(
                    "background sync: fetched {} state entries for program {}",
                    state_count,
                    program_id
                );
                return Ok(());
            }
        }

        versions.write().await.insert(program_id.clone(), 0);
        tracing::debug!(
            "background sync: no state found for program {}, marked as initialized",
            program_id
        );
        Ok(())
    }
}
