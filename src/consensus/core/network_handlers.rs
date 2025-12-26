//! Network event loop and message handlers for the consensus engine.

use super::DagEngine;
use crate::crypto::bls::{sign, verify, DST_RECEIPT};
use crate::crypto::hashing::hash_bytes;
use anyhow::{Context, Result};
use libp2p::PeerId;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};

impl DagEngine {
    pub async fn run(
        self: std::sync::Arc<Self>,
        mut events: mpsc::UnboundedReceiver<crate::network::NetworkEvent>,
    ) {
        let mut tick = interval(Duration::from_secs(5));
        let mut dht_announce_tick = interval(crate::network::dht::ANNOUNCE_INTERVAL);

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    if let Err(e) = self.periodic_sync().await {
                        tracing::warn!("periodic sync error: {e:?}");
                    }
                }
                _ = dht_announce_tick.tick() => {
                    if let Err(e) = self.periodic_dht_announce().await {
                        tracing::warn!("DHT announce error: {e:?}");
                    }
                }
                Some(event) = events.recv() => {
                    if let Err(e) = self.clone().handle_event(event).await {
                        tracing::warn!("event handling error: {e:?}");
                    }
                }
            }
        }
    }

    async fn periodic_dht_announce(&self) -> Result<()> {
        let objects = self.unified_store.list_objects()?;

        let mut announced = 0usize;

        for object in objects {
            if self
                .unified_store
                .get_manifest_by_object(&object.id)
                .ok()
                .flatten()
                .is_some()
            {
                self.network.provide(&object.id.0);
                announced += 1;
            }
        }

        tracing::debug!("announced {} objects to DHT", announced);
        Ok(())
    }

    async fn handle_event(self: Arc<Self>, event: crate::network::NetworkEvent) -> Result<()> {
        match event {
            crate::network::NetworkEvent::PeerConnected(peer_id) => {
                self.peers.write().await.insert(peer_id);
                tracing::info!("peer connected: {}", peer_id);
                let engine = self.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(crate::network::dht::ANNOUNCE_AFTER_CONNECT_DELAY).await;
                    let _ = engine.periodic_dht_announce().await;
                });
            }
            crate::network::NetworkEvent::PeerDisconnected(peer_id) => {
                self.peers.write().await.remove(&peer_id);
                tracing::info!("peer disconnected: {}", peer_id);
            }
            crate::network::NetworkEvent::Inbound(_peer_id, msg) => {
                self.handle_network_message(msg).await?;
            }
            crate::network::NetworkEvent::TransferRequest(peer_id, req, channel) => {
                self.clone()
                    .handle_transfer_request(peer_id, req, channel)
                    .await?;
            }
            crate::network::NetworkEvent::TransferResponse(_peer_id, resp) => {
                self.handle_transfer_response(resp).await?;
            }
            crate::network::NetworkEvent::ProvidersFound { key, peers, kind } => {
                self.handle_providers_found(key, peers, kind).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_providers_found(
        &self,
        key: Vec<u8>,
        peers: Vec<PeerId>,
        _kind: crate::network::ProviderKind,
    ) -> Result<()> {
        if key.len() == 32 {
            let mut object_id_bytes = [0u8; 32];
            object_id_bytes.copy_from_slice(&key);
            let object_id = crate::types::ObjectId(object_id_bytes);

            if peers.is_empty() {
                tracing::debug!("DHT query for {} returned 0 providers", object_id);
                return Ok(());
            }

            tracing::debug!(
                "DHT found {} providers for object {}: {:?}",
                peers.len(),
                object_id,
                peers.iter().map(|p| p.to_string()).collect::<Vec<_>>()
            );

            if let Some(ref distributor) = self.chunk_distributor {
                let mut provider_map = distributor.provider_map.write().await;
                let entry = provider_map.entry(object_id).or_insert_with(HashSet::new);

                for peer in peers {
                    entry.insert(peer);
                }

                tracing::debug!("added {} providers for object {}", entry.len(), object_id);
            }
        }
        Ok(())
    }

    async fn handle_transfer_response(
        &self,
        response: crate::network::TransferResponse,
    ) -> Result<()> {
        match response {
            crate::network::TransferResponse::Unified(unified_resp) => {
                use crate::network::unified_protocol::*;
                match unified_resp {
                    UnifiedResponse::Manifest(manifest_resp) => {
                        if let Some(manifest) = manifest_resp.manifest {
                            tracing::info!("received manifest for object {}", manifest.object_id);
                            let _ = self.unified_store.store_manifest(&manifest);
                            self.network.provide(&manifest.object_id.0);
                        }
                    }
                    UnifiedResponse::Chunk(chunk_resp) => {
                        if let Some(data) = chunk_resp.data {
                            let chunk = crate::types::Chunk {
                                id: chunk_resp.chunk_id,
                                data,
                            };
                            tracing::debug!("received chunk {}", chunk_resp.chunk_id);
                            let _ = self.unified_store.store_chunk(&chunk);
                        }
                    }
                    UnifiedResponse::ObjectMetadata(metadata_resp) => {
                        if let Some(object) = metadata_resp.metadata {
                            tracing::info!("received object metadata for {}", object.id);
                            let _ = self.unified_store.store_object(&object);
                        }
                    }
                    UnifiedResponse::FinalizedTransition(resp) => {
                        let crate::network::unified_protocol::FinalizedTransitionResponse {
                            program_id,
                            height,
                            bundle,
                        } = resp;
                        if let Some(bundle) = bundle {
                            if let Err(e) = self.handle_finalized_receipt(bundle).await {
                                tracing::debug!(
                                    "failed to apply finalized transition for program {} height {}: {e}",
                                    program_id,
                                    height
                                );
                            }
                        }
                    }
                    UnifiedResponse::ExecuteViaLeader(resp) => {
                        self.handle_execute_via_leader_response(resp).await?;
                    }
                    _ => {}
                }
            }
            crate::network::TransferResponse::StateResponse(state_resp) => {
                self.handle_state_response(state_resp).await?;
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_execute_via_leader_response(
        &self,
        resp: crate::network::unified_protocol::LeaderExecutionResponse,
    ) -> Result<()> {
        let sender = self
            .pending_leader_execs
            .write()
            .await
            .remove(&resp.request_id);
        let Some(sender) = sender else {
            return Ok(());
        };

        let msg = if let Some(outcome) = resp.outcome {
            super::engine::LeaderForwardResponse::Outcome(outcome)
        } else if let Some(redirect) = resp.redirect {
            super::engine::LeaderForwardResponse::Redirect(redirect)
        } else {
            super::engine::LeaderForwardResponse::Error(
                resp.error
                    .unwrap_or_else(|| "leader execution failed".to_string()),
            )
        };
        let _ = sender.send(msg);
        Ok(())
    }

    async fn handle_execute_via_leader_request(
        &self,
        req: crate::network::unified_protocol::LeaderExecutionRequest,
    ) -> crate::network::unified_protocol::UnifiedResponse {
        use crate::network::unified_protocol::*;

        let request_id = req.request_id;
        let program_id = req.program_id.clone();

        let committee = match self.program_catalog.get_manifest(&program_id) {
            Ok(Some(manifest)) => match manifest.committee {
                Some(c) => c,
                None => {
                    return UnifiedResponse::ExecuteViaLeader(LeaderExecutionResponse {
                        request_id,
                        outcome: None,
                        redirect: None,
                        error: Some("committee not set for program".to_string()),
                    });
                }
            },
            Ok(None) => {
                return UnifiedResponse::ExecuteViaLeader(LeaderExecutionResponse {
                    request_id,
                    outcome: None,
                    redirect: None,
                    error: Some("program not found".to_string()),
                });
            }
            Err(err) => {
                return UnifiedResponse::ExecuteViaLeader(LeaderExecutionResponse {
                    request_id,
                    outcome: None,
                    redirect: None,
                    error: Some(format!("loading program manifest failed: {err}")),
                });
            }
        };

        let next_height = match self.program_catalog.latest_state_commitment(&program_id) {
            Ok(Some(commit)) => commit.height.saturating_add(1),
            Ok(None) => 1,
            Err(err) => {
                return UnifiedResponse::ExecuteViaLeader(LeaderExecutionResponse {
                    request_id,
                    outcome: None,
                    redirect: None,
                    error: Some(format!("loading state commitment failed: {err}")),
                });
            }
        };

        let Some(leader) = leader_for_height(&committee, next_height) else {
            return UnifiedResponse::ExecuteViaLeader(LeaderExecutionResponse {
                request_id,
                outcome: None,
                redirect: None,
                error: Some("committee has no leader".to_string()),
            });
        };

        if leader != self.identity.node_id {
            return UnifiedResponse::ExecuteViaLeader(LeaderExecutionResponse {
                request_id,
                outcome: None,
                redirect: Some(leader),
                error: None,
            });
        }

        match self.submit_execution(&program_id, &req.calldata).await {
            Ok(outcome) => UnifiedResponse::ExecuteViaLeader(LeaderExecutionResponse {
                request_id,
                outcome: Some(outcome),
                redirect: None,
                error: None,
            }),
            Err(err) => UnifiedResponse::ExecuteViaLeader(LeaderExecutionResponse {
                request_id,
                outcome: None,
                redirect: None,
                error: Some(err.to_string()),
            }),
        }
    }

    async fn handle_network_message(&self, msg: crate::network::NetworkMessage) -> Result<()> {
        match msg {
            crate::network::NetworkMessage::UnifiedProtocol(unified_msg) => {
                self.handle_unified_protocol(unified_msg).await?;
            }
            crate::network::NetworkMessage::StateSync(state_sync) => {
                tracing::debug!(
                    "ignoring legacy StateSyncMessage for program {} from {} ({} writes)",
                    state_sync.program_id,
                    state_sync.executor_node,
                    state_sync.state_writes.len()
                );
            }
            crate::network::NetworkMessage::InventoryRequest => {
                let inv = self.build_inventory()?;
                self.network
                    .publisher
                    .send(crate::network::NetworkMessage::Inventory(inv))
                    .map_err(|_| anyhow::anyhow!("failed to send inventory"))?;
            }
            crate::network::NetworkMessage::Inventory(inv) => {
                {
                    let mut state = self.sync_state.write().await;
                    state.last_inventory = Some(inv);
                    state.last_seen = true;
                }
                self.refresh_sync_state().await?;
            }
            crate::network::NetworkMessage::Job(broadcast) => {
                let job_sync = { self.job_sync.read().await.clone() };
                if let Some(sync) = job_sync {
                    sync.handle_job_broadcast(broadcast).await?;
                }
            }
            _ => {
                tracing::debug!("received non-unified message");
            }
        }
        Ok(())
    }

    fn build_inventory(&self) -> Result<crate::network::DagInventory> {
        let objects = self.unified_store.list_objects()?;
        let mut programs = Vec::new();
        let mut blobs = Vec::new();

        for object in objects {
            let is_complete = self.unified_store.is_complete(&object.id).unwrap_or(false);

            match object.object_type {
                crate::types::ObjectType::WasmProgram { .. } => {
                    if is_complete {
                        programs.push(object.id.0);
                    }
                }
                crate::types::ObjectType::Blob { .. } => {
                    blobs.push(crate::network::BlobInventoryEntry {
                        id: object.id.0,
                        has_data: is_complete,
                        locations: Vec::new(),
                    });
                }
            }
        }

        Ok(crate::network::DagInventory {
            programs,
            program_bloom: None,
            blobs,
            executions: Vec::new(),
        })
    }

    async fn handle_unified_protocol(
        &self,
        msg: crate::network::unified_protocol::UnifiedProtocolMessage,
    ) -> Result<()> {
        match msg {
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectAnnouncement(
                announcement,
            ) => {
                tracing::info!(
                    "received object announcement: {} ({} bytes, {} chunks)",
                    announcement.object_id,
                    announcement.total_size,
                    announcement.chunk_count
                );

                if let Some(ref distributor) = self.chunk_distributor {
                    let _ = distributor.handle_announcement(announcement.clone()).await;
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadata(object) => {
                let already_have = self
                    .unified_store
                    .get_object_metadata(&object.id)
                    .ok()
                    .flatten()
                    .is_some();
                let is_complete = self.unified_store.is_complete(&object.id).unwrap_or(false);

                if !already_have {
                    tracing::info!("received new object metadata: {}", object.id);
                    if let Err(e) = self.unified_store.store_object(&object) {
                        tracing::warn!("failed to store object metadata: {}", e);
                        return Ok(());
                    }
                    tracing::info!("object {} metadata stored", object.id);
                } else {
                    tracing::debug!("already have metadata for object {}", object.id);
                }

                if !is_complete && !already_have {
                    if let Some(ref distributor) = self.chunk_distributor {
                        tracing::info!("object {} not complete, triggering fetch", object.id);

                        let distributor = distributor.clone();
                        let object_id = object.id;
                        tokio::spawn(async move {
                            let kind = match object.object_type {
                                crate::types::ObjectType::WasmProgram { .. } => {
                                    crate::network::ProviderKind::Program
                                }
                                crate::types::ObjectType::Blob { .. } => {
                                    crate::network::ProviderKind::Blob
                                }
                            };
                            match distributor.fetch_object(&object_id, kind).await {
                                Ok(data) => {
                                    tracing::info!(
                                        "successfully fetched object {} ({} bytes)",
                                        object_id,
                                        data.len()
                                    );
                                }
                                Err(e) => {
                                    tracing::warn!("failed to fetch object {}: {}", object_id, e);
                                }
                            }
                        });
                    }
                } else if is_complete {
                    tracing::debug!("object {} already complete locally", object.id);
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadataRequest(
                req,
            ) => {
                tracing::debug!("received metadata request for object {}", req.object_id);

                if let Ok(Some(object)) = self.unified_store.get_object_metadata(&req.object_id) {
                    if self
                        .unified_store
                        .is_complete(&req.object_id)
                        .unwrap_or(false)
                    {
                        tracing::info!("responding with metadata for {}", req.object_id);

                        let response_msg = crate::network::unified_protocol::UnifiedProtocolMessage::ObjectMetadata(object);
                        let network_msg =
                            crate::network::NetworkMessage::UnifiedProtocol(response_msg);
                        let _ = self.network.publisher.send(network_msg);
                    } else {
                        tracing::debug!(
                            "have metadata for {} but object incomplete",
                            req.object_id
                        );
                    }
                } else {
                    tracing::debug!("don't have metadata for {}", req.object_id);
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ManifestRequest(req) => {
                tracing::debug!("received manifest request for object {}", req.object_id);

                if let Ok(Some(manifest)) =
                    self.unified_store.get_manifest_by_object(&req.object_id)
                {
                    tracing::info!("responding with manifest for {}", req.object_id);

                    let response_msg =
                        crate::network::unified_protocol::UnifiedProtocolMessage::ManifestResponse(
                            manifest,
                        );
                    let network_msg = crate::network::NetworkMessage::UnifiedProtocol(response_msg);
                    let _ = self.network.publisher.send(network_msg);
                } else {
                    tracing::debug!("don't have manifest for {}", req.object_id);
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ProgramAnnouncement(
                announce,
            ) => {
                tracing::info!(
                    "received program manifest for {} (version {})",
                    announce.manifest.program_id,
                    announce.manifest.version
                );
                if let Err(e) = self.program_catalog.verify_initial_sync(&announce.manifest) {
                    tracing::warn!(
                        "manifest conflict for program {}: {}",
                        announce.manifest.program_id,
                        e
                    );
                    return Ok(());
                }
                if let Err(e) = self.program_catalog.store_manifest(&announce.manifest) {
                    tracing::warn!("failed to store program manifest: {}", e);
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::AggregatedReceipt(bundle) => {
                tracing::debug!(
                    "received aggregated receipt for program {} epoch {}",
                    bundle.receipt.receipt.program_id,
                    bundle.receipt.committee_epoch
                );
                if let Err(e) = self.handle_finalized_receipt(bundle).await {
                    tracing::warn!("failed to apply aggregated receipt: {}", e);
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::StateTransitionProposal(
                proposal,
            ) => {
                if let Err(e) = self.handle_state_transition_proposal(proposal).await {
                    tracing::debug!("proposal handling failed: {e}");
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::StateTransitionVote(vote) => {
                if let Err(e) = self.handle_state_transition_vote(vote).await {
                    tracing::debug!("vote handling failed: {e}");
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ProgramHead(head) => {
                if let Err(e) = self.handle_program_head(head).await {
                    tracing::debug!("program head handling failed: {e}");
                }
            }
            crate::network::unified_protocol::UnifiedProtocolMessage::ManifestResponse(
                manifest,
            ) => {
                tracing::info!(
                    "received manifest response for object {}",
                    manifest.object_id
                );

                if self
                    .unified_store
                    .get_manifest_by_object(&manifest.object_id)
                    .ok()
                    .flatten()
                    .is_none()
                {
                    let _ = self.unified_store.store_manifest(&manifest);
                    tracing::info!("stored manifest for {}", manifest.object_id);
                }
            }
        }
        Ok(())
    }

    async fn handle_state_transition_proposal(
        &self,
        proposal: crate::network::unified_protocol::StateTransitionProposal,
    ) -> Result<()> {
        let program_id = proposal.receipt.program_id.clone();

        let committee = self
            .program_catalog
            .get_manifest(&program_id)?
            .and_then(|m| m.committee)
            .ok_or_else(|| anyhow::anyhow!("missing committee for program {}", program_id))?;
        if proposal.committee_epoch != committee.epoch {
            return Ok(());
        }

        let Some(leader) = leader_for_height(&committee, proposal.receipt.height) else {
            return Ok(());
        };
        if proposal.receipt.executor != leader {
            tracing::debug!(
                "ignoring proposal for program {} height {}: executor {} is not leader {}",
                program_id,
                proposal.receipt.height,
                proposal.receipt.executor,
                leader
            );
            return Ok(());
        }

        let Some(my_index) = committee
            .members
            .iter()
            .position(|m| m.node == self.identity.node_id)
        else {
            return Ok(());
        };
        if committee.members[my_index].bls_public_key != self.bls_public {
            return Ok(());
        }

        let last_commit = self.program_catalog.latest_state_commitment(&program_id)?;
        let expected_height = last_commit.as_ref().map(|c| c.height + 1).unwrap_or(1);
        if proposal.receipt.height != expected_height {
            return Ok(());
        }

        let current_root = self.state_store.sparse_root_scoped(&program_id.0)?;
        if proposal.receipt.state_root_in != current_root {
            return Ok(());
        }

        let receipt_id = proposal.receipt.id();
        if let Some((height, voted_receipt)) = self.load_last_voted(&program_id).await? {
            if height == proposal.receipt.height && voted_receipt != receipt_id {
                return Ok(());
            }
            if height > proposal.receipt.height {
                return Ok(());
            }
        }

        if hash_bytes(&proposal.receipt.call.calldata) != proposal.receipt.inputs_hash {
            return Ok(());
        }

        let object_id = program_id.to_object_id();
        if !self.unified_store.is_complete(&object_id)? {
            if let Some(ref distributor) = self.chunk_distributor {
                let _ = distributor
                    .fetch_object(&object_id, crate::network::ProviderKind::Program)
                    .await;
            }
            if !self.unified_store.is_complete(&object_id)? {
                return Ok(());
            }
        }

        let outcome = match self
            .scheduler
            .execute(&program_id, &proposal.receipt.call.calldata)
            .await
        {
            Ok(o) => o,
            Err(_) => return Ok(()),
        };

        if outcome.state_writes != proposal.state_writes {
            return Ok(());
        }
        if outcome.state_root != proposal.receipt.state_root_out {
            return Ok(());
        }

        let digest = hash_bytes(&bincode::serde::encode_to_vec(
            &proposal.state_writes,
            bincode::config::standard(),
        )?);
        if digest != proposal.receipt.write_digest {
            return Ok(());
        }

        let predicted_root =
            compute_root_after_writes(&self.state_store, &program_id, &proposal.state_writes)?;
        if predicted_root != proposal.receipt.state_root_out {
            return Ok(());
        }

        let sig = sign(&receipt_id, &self.bls_secret, DST_RECEIPT)?;
        self.store_last_voted(&program_id, proposal.receipt.height, receipt_id)
            .await?;

        {
            let mut pending = self.pending_transitions.write().await;
            pending
                .entry(receipt_id)
                .or_insert_with(|| super::engine::PendingTransition {
                    committee: committee.clone(),
                    receipt: proposal.receipt.clone(),
                    state_writes: proposal.state_writes.clone(),
                    signatures: std::collections::HashMap::new(),
                    completion: None,
                })
                .signatures
                .insert(self.identity.node_id.clone(), sig.clone());
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

        self.maybe_finalize_pending_transition(receipt_id).await?;
        Ok(())
    }

    async fn handle_state_transition_vote(
        &self,
        vote: crate::network::unified_protocol::StateTransitionVote,
    ) -> Result<()> {
        let receipt_id = vote.receipt_id;
        let mut finalize: Option<super::engine::PendingTransition> = None;

        {
            let mut pending = self.pending_transitions.write().await;
            let Some(mut entry) = pending.remove(&receipt_id) else {
                return Ok(());
            };

            if vote.program_id != entry.receipt.program_id {
                pending.insert(receipt_id, entry);
                return Ok(());
            }
            if vote.committee_epoch != entry.committee.epoch {
                pending.insert(receipt_id, entry);
                return Ok(());
            }

            let Some(member) = entry
                .committee
                .members
                .iter()
                .find(|m| m.node == vote.signer)
            else {
                pending.insert(receipt_id, entry);
                return Ok(());
            };

            if entry.signatures.contains_key(&vote.signer) {
                pending.insert(receipt_id, entry);
                return Ok(());
            }

            if verify(
                &receipt_id,
                &member.bls_public_key,
                &vote.signature,
                DST_RECEIPT,
            )
            .is_err()
            {
                pending.insert(receipt_id, entry);
                return Ok(());
            }

            entry.signatures.insert(vote.signer.clone(), vote.signature);

            let signer_weight = signer_weight(&entry.committee, entry.signatures.keys());
            if signer_weight >= entry.committee.threshold as u64 {
                finalize = Some(entry);
            } else {
                pending.insert(receipt_id, entry);
            }
        }

        if let Some(entry) = finalize {
            self.finalize_transition(entry).await?;
        }

        Ok(())
    }

    async fn handle_program_head(
        &self,
        head: crate::network::unified_protocol::ProgramHead,
    ) -> Result<()> {
        let local_height = self
            .program_catalog
            .latest_state_commitment(&head.program_id)?
            .map(|c| c.height)
            .unwrap_or(0);

        if head.height <= local_height {
            return Ok(());
        }

        let next_height = local_height.saturating_add(1);
        if self
            .load_finalized_transition(&head.program_id, next_height)?
            .is_some()
        {
            let _ = self
                .apply_available_finalized_transitions(&head.program_id)
                .await;
            return Ok(());
        }

        let _ = self
            .request_finalized_transition(&head.program_id, next_height)
            .await;
        Ok(())
    }

    async fn apply_available_finalized_transitions(
        &self,
        program_id: &crate::types::ProgramId,
    ) -> Result<()> {
        loop {
            let expected_committee = self
                .program_catalog
                .get_manifest(program_id)?
                .and_then(|m| m.committee)
                .ok_or_else(|| anyhow::anyhow!("missing committee for program {}", program_id))?;

            let last_commit = self.program_catalog.latest_state_commitment(program_id)?;
            let expected_height = last_commit.as_ref().map(|c| c.height + 1).unwrap_or(1);

            let Some(bundle) = self.load_finalized_transition(program_id, expected_height)? else {
                return Ok(());
            };

            if bundle.committee != expected_committee {
                return Ok(());
            }

            self.ingest_aggregated_receipt(bundle.receipt.clone(), bundle.committee.clone())
                .await?;

            let current_root = self.state_store.sparse_root_scoped(&program_id.0)?;
            if bundle.receipt.receipt.state_root_in != current_root {
                return Ok(());
            }

            let digest = hash_bytes(&bincode::serde::encode_to_vec(
                &bundle.state_writes,
                bincode::config::standard(),
            )?);
            if digest != bundle.receipt.receipt.write_digest {
                return Ok(());
            }

            let predicted_root =
                compute_root_after_writes(&self.state_store, program_id, &bundle.state_writes)?;
            if predicted_root != bundle.receipt.receipt.state_root_out {
                return Ok(());
            }

            self.apply_state_writes(program_id, &bundle.state_writes)?;
            let applied_root = self.state_store.sparse_root_scoped(&program_id.0)?;
            if applied_root != bundle.receipt.receipt.state_root_out {
                return Ok(());
            }

            self.program_catalog
                .store_state_commitment(&crate::types::StateCommitment {
                    program_id: program_id.clone(),
                    height: bundle.receipt.receipt.height,
                    root: applied_root,
                    parent: Some(current_root),
                    state_delta_root: Some(bundle.receipt.receipt.write_digest),
                    timestamp_ms: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                })?;

            self.program_state_versions
                .write()
                .await
                .insert(program_id.clone(), bundle.receipt.receipt.height);
        }
    }

    pub(super) async fn maybe_finalize_pending_transition(
        &self,
        receipt_id: crate::types::ReceiptId,
    ) -> Result<()> {
        let entry = {
            let mut pending = self.pending_transitions.write().await;
            let Some(entry) = pending.get(&receipt_id) else {
                return Ok(());
            };
            let signer_weight = signer_weight(&entry.committee, entry.signatures.keys());
            if signer_weight < entry.committee.threshold as u64 {
                return Ok(());
            }
            pending.remove(&receipt_id)
        };

        if let Some(entry) = entry {
            self.finalize_transition(entry).await?;
        }
        Ok(())
    }

    async fn finalize_transition(&self, entry: super::engine::PendingTransition) -> Result<()> {
        let program_id = entry.receipt.program_id.clone();
        let receipt_id = entry.receipt.id();

        let mut signer_bitmap = vec![0u8; (entry.committee.members.len() + 7) / 8];
        let mut sigs = Vec::new();
        let mut signer_weight = 0u64;
        for (idx, member) in entry.committee.members.iter().enumerate() {
            let Some(sig) = entry.signatures.get(&member.node) else {
                continue;
            };
            signer_bitmap[idx / 8] |= 1u8 << (idx % 8);
            sigs.push(sig.clone());
            signer_weight = signer_weight.saturating_add(member.weight);
        }
        if signer_weight < entry.committee.threshold as u64 {
            return Ok(());
        }

        let aggregate_signature = crate::crypto::bls::aggregate_signatures(&sigs)?;
        let aggregated = crate::types::AggregatedReceipt {
            receipt: entry.receipt.clone(),
            committee_epoch: entry.committee.epoch,
            signer_bitmap,
            aggregate_signature,
            aggregate_public_key: entry.committee.aggregate_public_key.clone(),
        };
        let bundle = crate::network::AggregatedReceiptBundle {
            receipt: aggregated,
            committee: entry.committee.clone(),
            state_writes: entry.state_writes.clone(),
        };

        let apply_result = self.handle_finalized_receipt(bundle.clone()).await;
        if apply_result.is_ok() {
            let _ = self.broadcast_aggregated_receipt(bundle).await;
        }

        if let Some(done) = entry.completion {
            let _ = done.send(apply_result.map(|_| ()));
        }

        tracing::info!(
            "finalized transition for program {} receipt {}",
            program_id,
            hex::encode(receipt_id)
        );

        Ok(())
    }

    async fn handle_state_response(&self, state_resp: crate::network::StateResponse) -> Result<()> {
        let expected_root = if let Some(commitment) = self
            .program_catalog
            .latest_state_commitment(&state_resp.program_id)?
        {
            commitment.root
        } else if let Some(manifest) = self.program_catalog.get_manifest(&state_resp.program_id)? {
            manifest.initial_state_root
        } else {
            tracing::debug!(
                "ignoring state response for unknown program {}",
                state_resp.program_id
            );
            return Ok(());
        };

        if state_resp.state_root != expected_root {
            tracing::warn!(
                "ignoring state response for program {}: root mismatch (expected {}, got {})",
                state_resp.program_id,
                hex::encode(expected_root),
                hex::encode(state_resp.state_root)
            );
            return Ok(());
        }

        let computed_root = crate::merkle::sparse_merkle_root(&state_resp.state_entries);
        if computed_root != expected_root {
            tracing::warn!(
                "ignoring state response for program {}: entries do not match expected root",
                state_resp.program_id
            );
            return Ok(());
        }

        tracing::info!(
            "accepted state response for program {} ({} entries)",
            state_resp.program_id,
            state_resp.state_entries.len()
        );

        for (key, value) in &state_resp.state_entries {
            self.state_store
                .set_scoped(&state_resp.program_id.0, key, value)
                .context("applying state entry")?;
        }

        let local_root = self
            .state_store
            .sparse_root_scoped(&state_resp.program_id.0)?;
        if local_root != expected_root {
            tracing::warn!(
                "state sync applied but local root mismatched for program {}: expected {}, got {}",
                state_resp.program_id,
                hex::encode(expected_root),
                hex::encode(local_root)
            );
        }

        Ok(())
    }

    async fn handle_finalized_receipt(
        &self,
        bundle: crate::network::AggregatedReceiptBundle,
    ) -> Result<()> {
        let program_id = bundle.receipt.receipt.program_id.clone();

        let expected_committee = self
            .program_catalog
            .get_manifest(&program_id)?
            .and_then(|m| m.committee);
        let Some(expected_committee) = expected_committee else {
            tracing::warn!(
                "ignoring aggregated receipt for unknown program {} (missing committee)",
                program_id
            );
            return Ok(());
        };
        if expected_committee != bundle.committee {
            tracing::warn!(
                "ignoring aggregated receipt for program {} due to committee mismatch",
                program_id
            );
            return Ok(());
        }

        self.ingest_aggregated_receipt(bundle.receipt.clone(), bundle.committee.clone())
            .await?;

        self.store_finalized_transition(&bundle)?;

        let last_commit = self.program_catalog.latest_state_commitment(&program_id)?;
        let expected_height = last_commit.as_ref().map(|c| c.height + 1).unwrap_or(1);
        if bundle.receipt.receipt.height > expected_height {
            let _ = self
                .request_finalized_transition(&program_id, expected_height)
                .await;
        }

        self.apply_available_finalized_transitions(&program_id)
            .await?;
        Ok(())
    }

    async fn handle_transfer_request(
        self: Arc<Self>,
        peer_id: PeerId,
        req: crate::network::TransferRequest,
        channel: libp2p::request_response::ResponseChannel<crate::network::TransferResponse>,
    ) -> Result<()> {
        match req {
            crate::network::TransferRequest::Unified(unified_req) => match unified_req {
                crate::network::unified_protocol::UnifiedRequest::ExecuteViaLeader(exec_req) => {
                    let engine = self.clone();
                    tokio::spawn(async move {
                        let response = engine.handle_execute_via_leader_request(exec_req).await;
                        engine.network.respond_transfer(
                            channel,
                            crate::network::TransferResponse::Unified(response),
                        );
                    });
                }
                other => {
                    let response = self.handle_unified_request(other).await?;
                    self.network.respond_transfer(
                        channel,
                        crate::network::TransferResponse::Unified(response),
                    );
                }
            },
            crate::network::TransferRequest::StateRequest(state_req) => {
                let response = self.handle_state_request(state_req).await?;
                self.network.respond_transfer(
                    channel,
                    crate::network::TransferResponse::StateResponse(response),
                );
            }
            _ => {
                tracing::debug!("received non-unified transfer request from {}", peer_id);
                self.network
                    .respond_transfer(channel, crate::network::TransferResponse::Ack);
            }
        }
        Ok(())
    }

    async fn handle_state_request(
        &self,
        req: crate::network::StateRequest,
    ) -> Result<crate::network::StateResponse> {
        let state_entries = self.state_store.get_all_scoped(&req.program_id.0)?;
        let state_root = self.state_store.sparse_root_scoped(&req.program_id.0)?;

        tracing::info!(
            "responding to state request for program {} ({} entries, root: {})",
            req.program_id,
            state_entries.len(),
            hex::encode(state_root)
        );

        Ok(crate::network::StateResponse {
            program_id: req.program_id,
            state_root,
            state_entries,
        })
    }

    async fn handle_unified_request(
        &self,
        req: crate::network::unified_protocol::UnifiedRequest,
    ) -> Result<crate::network::unified_protocol::UnifiedResponse> {
        use crate::network::unified_protocol::*;

        match req {
            UnifiedRequest::GetChunk(chunk_req) => {
                let chunk_data = self.unified_store.get_chunk(&chunk_req.chunk_id)?;
                Ok(UnifiedResponse::Chunk(ChunkResponse {
                    chunk_id: chunk_req.chunk_id,
                    data: chunk_data.map(|c| c.data),
                }))
            }
            UnifiedRequest::GetManifest(manifest_req) => {
                let manifest = self
                    .unified_store
                    .get_manifest_by_object(&manifest_req.object_id)?;
                Ok(UnifiedResponse::Manifest(ManifestResponse { manifest }))
            }
            UnifiedRequest::GetChunks(batch_req) => {
                let chunks: Vec<_> = batch_req
                    .chunk_ids
                    .iter()
                    .map(|cid| {
                        let data = self
                            .unified_store
                            .get_chunk(cid)
                            .ok()
                            .flatten()
                            .map(|c| c.data);
                        (*cid, data)
                    })
                    .collect();
                Ok(UnifiedResponse::Chunks(BatchChunkResponse { chunks }))
            }
            UnifiedRequest::GetObjectAvailability(avail_req) => {
                let has_object = self
                    .unified_store
                    .is_complete(&avail_req.object_id)
                    .unwrap_or(false);
                let manifest = self
                    .unified_store
                    .get_manifest_by_object(&avail_req.object_id)
                    .ok()
                    .flatten();
                let has_manifest = manifest.is_some();
                let metadata = self
                    .unified_store
                    .get_object_metadata(&avail_req.object_id)
                    .ok()
                    .flatten();

                let (available_chunks, missing_chunks) = if let Some(ref m) = manifest {
                    let missing = self.unified_store.get_missing_chunks(m).unwrap_or_default();
                    let available: Vec<_> = m
                        .chunks
                        .iter()
                        .filter(|c| !missing.contains(&c.chunk_id))
                        .map(|c| c.chunk_id)
                        .collect();
                    (available, missing)
                } else {
                    (Vec::new(), Vec::new())
                };

                Ok(UnifiedResponse::ObjectAvailability(
                    ObjectAvailabilityResponse {
                        object_id: avail_req.object_id,
                        has_object,
                        has_manifest,
                        available_chunks,
                        missing_chunks,
                        metadata,
                    },
                ))
            }
            UnifiedRequest::GetObjectMetadata(metadata_req) => {
                let metadata = self
                    .unified_store
                    .get_object_metadata(&metadata_req.object_id)
                    .ok()
                    .flatten();
                Ok(UnifiedResponse::ObjectMetadata(ObjectMetadataResponse {
                    object_id: metadata_req.object_id,
                    metadata,
                }))
            }
            UnifiedRequest::GetFinalizedTransition(req) => {
                let bundle = self.load_finalized_transition(&req.program_id, req.height)?;
                Ok(UnifiedResponse::FinalizedTransition(
                    FinalizedTransitionResponse {
                        program_id: req.program_id,
                        height: req.height,
                        bundle,
                    },
                ))
            }
            UnifiedRequest::ExecuteViaLeader(req) => Ok(UnifiedResponse::ExecuteViaLeader(
                crate::network::unified_protocol::LeaderExecutionResponse {
                    request_id: req.request_id,
                    outcome: None,
                    redirect: None,
                    error: Some("ExecuteViaLeader is handled asynchronously".to_string()),
                },
            )),
        }
    }
}

fn compute_root_after_writes(
    state_store: &crate::storage::StateStore,
    program_id: &crate::types::ProgramId,
    writes: &[crate::types::StateWrite],
) -> Result<[u8; 32]> {
    let pairs = state_store.get_all_scoped(&program_id.0)?;
    let mut map: std::collections::HashMap<Vec<u8>, Vec<u8>> = pairs.into_iter().collect();

    for write in writes {
        match &write.value {
            Some(v) => {
                map.insert(write.key.clone(), v.clone());
            }
            None => {
                map.remove(&write.key);
            }
        }
    }

    let merged: Vec<(Vec<u8>, Vec<u8>)> = map.into_iter().collect();
    Ok(crate::merkle::sparse_merkle_root(&merged))
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

fn signer_weight<'a>(
    committee: &crate::types::CommitteeCertificate,
    signers: impl Iterator<Item = &'a crate::types::NodeId>,
) -> u64 {
    let mut weight = 0u64;
    for signer in signers {
        if let Some(member) = committee.members.iter().find(|m| &m.node == signer) {
            weight = weight.saturating_add(member.weight);
        }
    }
    weight
}
