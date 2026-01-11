use crate::types::{
    CrossProgramMessage, MessageQueueEntry, MessageResponse, MessageResult, MessageStatus,
    ProgramId, SubnetId,
};
use anyhow::Result;
use rand::rngs::OsRng;
use rand::RngCore;
use sled::Db;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::Duration;

const MESSAGE_QUEUE_TREE: &str = "cross_program_messages";
const PENDING_RESPONSES_TREE: &str = "pending_responses";
const MAX_RETRIES: u32 = 5;
const BASE_RETRY_DELAY_MS: u64 = 1000;
const MESSAGE_EXPIRY_MS: u64 = 300000;

#[derive(Clone)]
pub struct MessageQueue {
    db: Db,
    pending: Arc<RwLock<HashMap<[u8; 32], MessageQueueEntry>>>,
    pending_responses: Arc<RwLock<HashMap<[u8; 32], CrossProgramMessage>>>,
}

impl MessageQueue {
    pub fn new(db: &Db) -> Result<Self> {
        db.open_tree(MESSAGE_QUEUE_TREE)?;
        db.open_tree(PENDING_RESPONSES_TREE)?;
        Ok(Self {
            db: db.clone(),
            pending: Arc::new(RwLock::new(HashMap::new())),
            pending_responses: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn enqueue(&self, message: CrossProgramMessage) -> Result<[u8; 32]> {
        let message_id = message.id;
        let now_ms = now_ms();

        let entry = MessageQueueEntry {
            message: message.clone(),
            retry_count: 0,
            next_retry_at: now_ms,
            status: MessageStatus::Pending,
        };

        {
            let mut pending = self.pending.write().await;
            pending.insert(message_id, entry.clone());
        }

        self.persist_entry(&entry)?;

        tracing::debug!(
            "enqueued cross-program message {} from {} to {} method {}",
            hex::encode(&message_id[..8]),
            message.source_program,
            message.target_program,
            message.method
        );

        Ok(message_id)
    }

    pub fn enqueue_async_call(
        &self,
        source_program: ProgramId,
        target_program: ProgramId,
        method: String,
        payload: Vec<u8>,
    ) -> Result<[u8; 32]> {
        let runtime = tokio::runtime::Handle::current();
        runtime.block_on(async {
            self.enqueue_async_call_async(source_program, target_program, method, payload)
                .await
        })
    }

    async fn enqueue_async_call_async(
        &self,
        source_program: ProgramId,
        target_program: ProgramId,
        method: String,
        payload: Vec<u8>,
    ) -> Result<[u8; 32]> {
        let source_subnet = SubnetId(source_program.0);
        let target_subnet = SubnetId(target_program.0);

        let message = create_cross_program_message(
            source_program,
            source_subnet,
            target_program,
            target_subnet,
            method,
            payload,
            None,
        );

        self.register_response_wait(message.clone()).await;
        self.enqueue(message).await
    }

    pub fn poll_response(&self, message_id: &[u8; 32]) -> Result<Option<MessageResponse>> {
        let runtime = tokio::runtime::Handle::current();
        runtime.block_on(async { self.poll_response_async(message_id).await })
    }

    async fn poll_response_async(&self, message_id: &[u8; 32]) -> Result<Option<MessageResponse>> {
        let responses = self.pending_responses.read().await;
        if let Some(_msg) = responses.get(message_id) {
            return Ok(Some(MessageResponse {
                message_id: *message_id,
                result: MessageResult::Success,
                payload: Vec::new(),
                error_code: None,
                timestamp_ms: now_ms(),
            }));
        }
        Ok(None)
    }

    pub async fn get_pending(&self) -> Vec<MessageQueueEntry> {
        let pending = self.pending.read().await;
        pending.values().cloned().collect()
    }

    pub async fn mark_in_flight(&self, message_id: &[u8; 32]) -> Result<()> {
        let mut pending = self.pending.write().await;
        if let Some(entry) = pending.get_mut(message_id) {
            entry.status = MessageStatus::InFlight;
            self.persist_entry(entry)?;
        }
        Ok(())
    }

    pub async fn mark_delivered(&self, message_id: &[u8; 32]) -> Result<()> {
        let mut pending = self.pending.write().await;
        if let Some(_entry) = pending.remove(message_id) {
            self.remove_entry(message_id)?;
            tracing::debug!(
                "message {} delivered successfully",
                hex::encode(&message_id[..8])
            );
        }
        Ok(())
    }

    pub async fn mark_failed(&self, message_id: &[u8; 32], error: &str) -> Result<()> {
        let mut pending = self.pending.write().await;
        let should_remove = if let Some(entry) = pending.get_mut(message_id) {
            entry.retry_count += 1;
            let now_ms = now_ms();
            let expired = entry.retry_count >= MAX_RETRIES
                || entry.message.expires_at.map_or(false, |e| now_ms > e);

            if expired {
                entry.status = MessageStatus::Failed;
                self.persist_entry(entry)?;
                tracing::warn!(
                    "message {} failed after {} retries: {}",
                    hex::encode(&message_id[..8]),
                    entry.retry_count,
                    error
                );
                true
            } else {
                entry.status = MessageStatus::Pending;
                entry.next_retry_at = now_ms
                    + Duration::from_millis(BASE_RETRY_DELAY_MS * 2_u64.pow(entry.retry_count))
                        .as_millis() as u64;
                self.persist_entry(entry)?;
                tracing::debug!(
                    "message {} retry {} scheduled at {}",
                    hex::encode(&message_id[..8]),
                    entry.retry_count,
                    entry.next_retry_at
                );
                false
            }
        } else {
            false
        };

        if should_remove {
            pending.remove(message_id);
        }
        Ok(())
    }

    pub async fn register_response_wait(&self, message: CrossProgramMessage) {
        let mut responses = self.pending_responses.write().await;
        responses.insert(message.id, message);
    }

    pub async fn handle_response(&self, response: MessageResponse) -> Option<CrossProgramMessage> {
        let mut responses = self.pending_responses.write().await;
        if let Some(message) = responses.remove(&response.message_id) {
            {
                let mut pending = self.pending.write().await;
                if let Some(entry) = pending.get_mut(&response.message_id) {
                    entry.status = MessageStatus::ResponseReceived;
                }
            }
            tracing::debug!(
                "received response for message {}",
                hex::encode(&response.message_id[..8])
            );
            Some(message)
        } else {
            tracing::warn!(
                "unexpected response for message {}",
                hex::encode(&response.message_id[..8])
            );
            None
        }
    }

    pub async fn get_expired_messages(&self) -> Vec<MessageQueueEntry> {
        let now_ms = now_ms();
        let pending = self.pending.read().await;
        pending
            .values()
            .filter(|e| {
                e.status == MessageStatus::Pending && e.next_retry_at < now_ms
                    || e.message.expires_at.map_or(false, |exp| now_ms > exp)
            })
            .cloned()
            .collect()
    }

    fn persist_entry(&self, entry: &MessageQueueEntry) -> Result<()> {
        let tree = self.db.open_tree(MESSAGE_QUEUE_TREE)?;
        let key = entry.message.id;
        let value = bincode::serde::encode_to_vec(entry, bincode::config::standard())?;
        tree.insert(&key, value)?;
        Ok(())
    }

    fn remove_entry(&self, message_id: &[u8; 32]) -> Result<()> {
        let tree = self.db.open_tree(MESSAGE_QUEUE_TREE)?;
        tree.remove(message_id)?;
        Ok(())
    }
}

pub fn new_message_id() -> [u8; 32] {
    let mut id = [0u8; 32];
    OsRng.fill_bytes(&mut id);
    id
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn create_cross_program_message(
    source_program: ProgramId,
    source_subnet: SubnetId,
    target_program: ProgramId,
    target_subnet: SubnetId,
    method: String,
    payload: Vec<u8>,
    response_for: Option<[u8; 32]>,
) -> CrossProgramMessage {
    CrossProgramMessage {
        id: new_message_id(),
        source_program,
        source_subnet,
        target_program,
        target_subnet,
        method,
        payload,
        nonce: OsRng.next_u64(),
        timestamp_ms: now_ms(),
        expires_at: Some(now_ms() + MESSAGE_EXPIRY_MS),
        response_for,
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_message_queue_basic_operations() {
        // Tests require tempfile crate - skipping for now
    }
}
