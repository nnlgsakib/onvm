//! Ping / Pong keep-alive protocol.
//!
//! Uses a lightweight request-response round-trip to detect dead peers and measure latency.

use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};

pub const PING_PROTOCOL: &str = "/onvm/ping/1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingRequest {
    pub nonce: [u8; 32],
    pub sent_at_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongResponse {
    pub nonce: [u8; 32],
    pub received_at_ms: u64,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub fn new_ping() -> PingRequest {
    let mut nonce = [0u8; 32];
    OsRng.fill_bytes(&mut nonce);
    PingRequest {
        nonce,
        sent_at_ms: now_ms(),
    }
}
