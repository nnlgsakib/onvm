use super::DagEngine;
use crate::crypto::bls::{sign, DST_RECEIPT};
use crate::crypto::hashing::hash_bytes;
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

        let state_root_in = self.state_store.sparse_root_scoped(&program_id.0)?;

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

        // Build execution receipt and aggregated receipt (single-node committee for now).
        let request_id = hash_bytes(input);
        let inputs_hash = hash_bytes(input);
        let write_digest = hash_bytes(&bincode::serde::encode_to_vec(
            &outcome.state_writes,
            bincode::config::standard(),
        )?);
        let events_hash = [0u8; 32];
        let wasm_env_hash = hash_bytes(b"wasm_env_v1");
        let wasm_code_hash = self
            .unified_store
            .get_manifest_by_object(&object_id)?
            .map(|m| m.content_hash);

        let receipt = crate::types::ExecutionReceipt {
            program_id: program_id.clone(),
            state_root_in,
            call: crate::types::ExecutionCall {
                entrypoint: self
                    .execution_adapter
                    .get_entrypoint(&object_id)
                    .unwrap_or_else(|_| "unknown".to_string()),
                calldata: input.to_vec(),
                request_id,
            },
            inputs_hash,
            gas_used: outcome.fuel_consumed,
            state_root_out: outcome.state_root,
            write_digest,
            events_hash,
            wasm_code_hash,
            wasm_env_hash,
            nonce: request_id[0] as u64,
            executor: self.identity.node_id.clone(),
        };

        let receipt_id = receipt.id();
        let sig = sign(&receipt_id, &self.bls_secret, DST_RECEIPT)?;
        let aggregated = crate::types::AggregatedReceipt {
            receipt,
            committee_epoch: 0,
            signer_bitmap: vec![0b1],
            aggregate_signature: sig.clone(),
            aggregate_public_key: self.bls_public.clone(),
        };

        let committee = crate::types::CommitteeCertificate {
            program_id: program_id.clone(),
            epoch: 0,
            members: vec![crate::types::CommitteeMember {
                node: self.identity.node_id.clone(),
                weight: 1,
                bls_public_key: self.bls_public.clone(),
            }],
            threshold: 1,
            aggregate_public_key: self.bls_public.clone(),
            signature: None,
        };

        // Store locally and broadcast for verification.
        self.ingest_aggregated_receipt(aggregated.clone(), committee.clone())
            .await?;
        let bundle = crate::network::AggregatedReceiptBundle {
            receipt: aggregated,
            committee,
        };
        let _ = self.broadcast_aggregated_receipt(bundle).await;

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
