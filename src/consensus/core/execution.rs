//! Execution path that produces receipts and finalized state deltas.

use super::DagEngine;
use crate::crypto::bls::{sign, DST_RECEIPT};
use crate::crypto::hashing::hash_bytes;
use anyhow::{anyhow, Result};
use libp2p::identity as libp2p_identity;
use libp2p::PeerId;
use rand::rngs::OsRng;
use rand::RngCore;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::sync::RwLock;
use tokio::time::Duration;

impl DagEngine {
    pub async fn submit_execution(
        &self,
        program_id: &crate::types::ProgramId,
        input: &[u8],
    ) -> Result<crate::wasm_runtime::ExecutionOutcome> {
        let committee = self
            .program_catalog
            .get_manifest(program_id)?
            .and_then(|m| m.committee)
            .ok_or_else(|| anyhow!("missing committee for program {}", program_id))?;

        let next_height_hint = self
            .program_catalog
            .latest_state_commitment(program_id)?
            .map(|c| c.height.saturating_add(1))
            .unwrap_or(1);
        let Some(leader_hint) = leader_for_height(&committee, next_height_hint) else {
            return Err(anyhow!(
                "committee has no leader for height {}",
                next_height_hint
            ));
        };

        if leader_hint != self.identity.node_id {
            if let Some(outcome) = self
                .forward_execution_to_leader(program_id, input, &committee, &leader_hint)
                .await?
            {
                return Ok(outcome);
            }
        }

        let program_lock = self.program_lock(program_id).await;
        let _guard = program_lock.lock().await;

        let next_height = self
            .program_catalog
            .latest_state_commitment(program_id)?
            .map(|c| c.height.saturating_add(1))
            .unwrap_or(1);
        let Some(leader) = leader_for_height(&committee, next_height) else {
            return Err(anyhow!(
                "committee has no leader for height {}",
                next_height
            ));
        };

        if leader != self.identity.node_id {
            if let Some(outcome) = self
                .forward_execution_to_leader(program_id, input, &committee, &leader)
                .await?
            {
                return Ok(outcome);
            }
        }

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

        // Build execution receipt and propose it to the committee.
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
            height: next_height,
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

        let my_index = committee
            .members
            .iter()
            .position(|m| m.node == self.identity.node_id)
            .ok_or_else(|| anyhow!("node is not in committee for program {}", program_id))?;
        if committee.members[my_index].bls_public_key != self.bls_public {
            return Err(anyhow!(
                "local BLS public key does not match committee entry for {}",
                self.identity.node_id
            ));
        }

        let receipt_id = receipt.id();
        let (done_tx, done_rx) = oneshot::channel::<Result<()>>();

        {
            let mut pending = self.pending_transitions.write().await;
            pending.insert(
                receipt_id,
                super::engine::PendingTransition {
                    committee: committee.clone(),
                    receipt: receipt.clone(),
                    state_writes: outcome.state_writes.clone(),
                    signatures: HashMap::new(),
                    completion: Some(done_tx),
                },
            );
        }

        let proposal = crate::network::unified_protocol::StateTransitionProposal {
            receipt: receipt.clone(),
            committee_epoch: committee.epoch,
            state_writes: outcome.state_writes.clone(),
        };
        let msg = crate::network::UnifiedProtocolMessage::StateTransitionProposal(proposal);
        let net_msg = crate::network::NetworkMessage::UnifiedProtocol(msg);
        let _ = self.network.publisher.send(net_msg);

        let sig = sign(&receipt_id, &self.bls_secret, DST_RECEIPT)?;
        self.store_last_voted(program_id, receipt.height, receipt_id)
            .await?;
        {
            let mut pending = self.pending_transitions.write().await;
            if let Some(entry) = pending.get_mut(&receipt_id) {
                entry
                    .signatures
                    .insert(self.identity.node_id.clone(), sig.clone());
            }
        }

        let vote = crate::network::unified_protocol::StateTransitionVote {
            program_id: program_id.clone(),
            committee_epoch: committee.epoch,
            receipt_id,
            signer: self.identity.node_id.clone(),
            signature: sig,
        };
        let msg = crate::network::UnifiedProtocolMessage::StateTransitionVote(vote);
        let net_msg = crate::network::NetworkMessage::UnifiedProtocol(msg);
        let _ = self.network.publisher.send(net_msg);

        let _ = self.maybe_finalize_pending_transition(receipt_id).await;

        match tokio::time::timeout(Duration::from_secs(20), done_rx).await {
            Ok(Ok(Ok(()))) => Ok(outcome),
            Ok(Ok(Err(err))) => Err(err),
            Ok(Err(_)) => Err(anyhow!("consensus completion dropped for {}", program_id)),
            Err(_) => {
                self.pending_transitions.write().await.remove(&receipt_id);
                Err(anyhow!("consensus timed out for program {}", program_id))
            }
        }
    }

    async fn forward_execution_to_leader(
        &self,
        program_id: &crate::types::ProgramId,
        input: &[u8],
        committee: &crate::types::CommitteeCertificate,
        leader_hint: &crate::types::NodeId,
    ) -> Result<Option<crate::wasm_runtime::ExecutionOutcome>> {
        let mut queue: std::collections::VecDeque<crate::types::NodeId> =
            std::collections::VecDeque::new();
        queue.push_back(leader_hint.clone());
        for member in &committee.members {
            if member.node != *leader_hint {
                queue.push_back(member.node.clone());
            }
        }

        let mut attempted: std::collections::HashSet<crate::types::NodeId> =
            std::collections::HashSet::new();
        let mut attempts = 0usize;

        while let Some(target) = queue.pop_front() {
            if !attempted.insert(target.clone()) {
                continue;
            }
            attempts = attempts.saturating_add(1);
            if attempts > committee.members.len().saturating_add(4) {
                break;
            }

            if target == self.identity.node_id {
                return Ok(None);
            }

            let resp = match self
                .request_leader_execution(&target, program_id, input)
                .await
            {
                Ok(r) => r,
                Err(_) => continue,
            };

            match resp {
                super::engine::LeaderForwardResponse::Outcome(outcome) => {
                    return Ok(Some(outcome));
                }
                super::engine::LeaderForwardResponse::Redirect(leader) => {
                    queue.push_front(leader);
                }
                super::engine::LeaderForwardResponse::Error(msg) => {
                    return Err(anyhow!(msg));
                }
            }
        }

        Err(anyhow!(
            "unable to forward execution for program {} to a leader",
            program_id
        ))
    }

    async fn request_leader_execution(
        &self,
        target: &crate::types::NodeId,
        program_id: &crate::types::ProgramId,
        input: &[u8],
    ) -> Result<super::engine::LeaderForwardResponse> {
        let peer_id = peer_id_for_node_id(target)?;
        if peer_id == self.network.peer_id {
            return Ok(super::engine::LeaderForwardResponse::Redirect(
                self.identity.node_id.clone(),
            ));
        }

        let mut request_id = [0u8; 32];
        OsRng.fill_bytes(&mut request_id);
        let (tx, rx) = oneshot::channel();

        self.pending_leader_execs
            .write()
            .await
            .insert(request_id, tx);

        let req = crate::network::unified_protocol::LeaderExecutionRequest {
            request_id,
            program_id: program_id.clone(),
            calldata: input.to_vec(),
        };
        let unified_req = crate::network::UnifiedRequest::ExecuteViaLeader(req);
        self.network.request_transfer(
            peer_id,
            crate::network::TransferRequest::Unified(unified_req),
        );

        match tokio::time::timeout(Duration::from_secs(15), rx).await {
            Ok(Ok(resp)) => Ok(resp),
            Ok(Err(_)) => Err(anyhow!("leader execution response dropped")),
            Err(_) => {
                self.pending_leader_execs.write().await.remove(&request_id);
                Err(anyhow!("leader execution request timed out"))
            }
        }
    }

    pub(super) fn apply_state_writes(
        &self,
        program_id: &crate::types::ProgramId,
        writes: &[crate::types::StateWrite],
    ) -> Result<()> {
        for write in writes {
            match &write.value {
                Some(value) => self
                    .state_store
                    .set_scoped(&program_id.0, &write.key, value)?,
                None => self.state_store.delete_scoped(&program_id.0, &write.key)?,
            }
        }
        Ok(())
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

fn leader_for_height(
    committee: &crate::types::CommitteeCertificate,
    height: u64,
) -> Option<crate::types::NodeId> {
    if committee.members.is_empty() || height == 0 {
        return None;
    }
    let idx = (height.saturating_sub(1) as usize) % committee.members.len();
    Some(committee.members[idx].node.clone())
}

fn peer_id_for_node_id(node: &crate::types::NodeId) -> Result<PeerId> {
    let pk = libp2p_identity::ed25519::PublicKey::try_from_bytes(&node.0)
        .map_err(|e| anyhow!("invalid ed25519 public key for peer id: {e:?}"))?;
    let public = libp2p_identity::PublicKey::from(pk);
    Ok(PeerId::from(public))
}
