//! Centralized runtime configuration for ONVM nodes.
//!
//! This holds genesis/static chain parameters, block production parameters,
//! and networking defaults so components share a single source of truth.

use anyhow::Result;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct GenesisConfig {
    /// Optional precomputed global state root; if absent, an empty root is assumed.
    pub state_root: Option<[u8; 32]>,
}

#[derive(Clone, Debug)]
pub struct NetworkConfig {
    /// Minimum number of peers (excluding self) required to produce blocks.
    pub min_peers: usize,
    /// Default bootnodes to attempt dialing on startup.
    pub bootnodes: Vec<String>,
    /// Enable local mDNS peer discovery (should stay off in production).
    pub enable_mdns: bool,
    /// Require encrypted transports (noise/tls); no plaintext fallback.
    pub require_encryption: bool,
    /// Maximum inbound connections allowed (hard cap).
    pub max_inbound_connections: usize,
    /// Maximum concurrent inbound request/response streams.
    pub max_inbound_streams: usize,
    /// Maximum gossipsub message size in bytes.
    pub max_gossip_bytes: usize,
}

#[derive(Clone, Debug)]
pub struct RpcAuthConfig {
    pub enable: bool,
    /// Public identifier clients send with requests.
    pub project_id: String,
    /// Path to the encrypted primary secret (relative to data dir or absolute).
    pub secret_path: Option<String>,
    /// Optional path to a secondary encrypted secret for rotation.
    pub secondary_secret_path: Option<String>,
    /// Allowed clock skew in milliseconds.
    pub skew_ms: u64,
    /// Nonce TTL in milliseconds.
    pub nonce_ttl_ms: u64,
    /// Max nonces to track for replay protection.
    pub nonce_capacity: usize,
    /// Optional list of allowed HTTP methods (uppercase). Empty means all.
    pub allowed_methods: Vec<String>,
    /// Optional per-minute rate limit; zero disables.
    pub rate_limit_per_minute: u64,
}

#[derive(Clone, Debug)]
pub struct OnvmConfig {
    pub genesis: GenesisConfig,
    pub network: NetworkConfig,
    pub rpc_auth: RpcAuthConfig,
}

impl OnvmConfig {
    pub fn to_toml(&self) -> toml::Value {
        let mut root = toml::map::Map::new();

        let mut genesis = toml::map::Map::new();
        genesis.insert(
            "state_root".into(),
            self.genesis
                .state_root
                .as_ref()
                .map(|r| toml::Value::String(hex::encode(r)))
                .unwrap_or_else(|| toml::Value::String(String::new())),
        );
        root.insert("genesis".into(), toml::Value::Table(genesis));

        let mut network = toml::map::Map::new();
        network.insert(
            "min_peers".into(),
            toml::Value::Integer(self.network.min_peers as i64),
        );
        network.insert(
            "bootnodes".into(),
            toml::Value::Array(
                self.network
                    .bootnodes
                    .iter()
                    .cloned()
                    .map(toml::Value::String)
                    .collect(),
            ),
        );
        network.insert(
            "enable_mdns".into(),
            toml::Value::Boolean(self.network.enable_mdns),
        );
        network.insert(
            "require_encryption".into(),
            toml::Value::Boolean(self.network.require_encryption),
        );
        network.insert(
            "max_inbound_connections".into(),
            toml::Value::Integer(self.network.max_inbound_connections as i64),
        );
        network.insert(
            "max_inbound_streams".into(),
            toml::Value::Integer(self.network.max_inbound_streams as i64),
        );
        network.insert(
            "max_gossip_bytes".into(),
            toml::Value::Integer(self.network.max_gossip_bytes as i64),
        );
        root.insert("network".into(), toml::Value::Table(network));

        let mut rpc_auth = toml::map::Map::new();
        rpc_auth.insert(
            "enable_rpc_auth".into(),
            toml::Value::Boolean(self.rpc_auth.enable),
        );
        rpc_auth.insert(
            "project_id".into(),
            toml::Value::String(self.rpc_auth.project_id.clone()),
        );
        rpc_auth.insert(
            "secret_path".into(),
            self.rpc_auth
                .secret_path
                .as_ref()
                .map(|s| toml::Value::String(s.clone()))
                .unwrap_or_else(|| toml::Value::String(String::new())),
        );
        rpc_auth.insert(
            "secondary_secret_path".into(),
            self.rpc_auth
                .secondary_secret_path
                .as_ref()
                .map(|s| toml::Value::String(s.clone()))
                .unwrap_or_else(|| toml::Value::String(String::new())),
        );
        rpc_auth.insert(
            "skew_ms".into(),
            toml::Value::Integer(self.rpc_auth.skew_ms as i64),
        );
        rpc_auth.insert(
            "nonce_ttl_ms".into(),
            toml::Value::Integer(self.rpc_auth.nonce_ttl_ms as i64),
        );
        rpc_auth.insert(
            "nonce_capacity".into(),
            toml::Value::Integer(self.rpc_auth.nonce_capacity as i64),
        );
        rpc_auth.insert(
            "allowed_methods".into(),
            toml::Value::Array(
                self.rpc_auth
                    .allowed_methods
                    .iter()
                    .cloned()
                    .map(toml::Value::String)
                    .collect(),
            ),
        );
        rpc_auth.insert(
            "rate_limit_per_minute".into(),
            toml::Value::Integer(self.rpc_auth.rate_limit_per_minute as i64),
        );
        root.insert("rpc_auth".into(), toml::Value::Table(rpc_auth));

        toml::Value::Table(root)
    }
}

impl serde::Serialize for OnvmConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("OnvmConfig", 3)?;
        st.serialize_field(
            "genesis_state_root",
            &self.genesis.state_root.as_ref().map(hex::encode),
        )?;
        st.serialize_field("min_peers", &self.network.min_peers)?;
        st.serialize_field("enable_mdns", &self.network.enable_mdns)?;
        st.serialize_field("require_encryption", &self.network.require_encryption)?;
        st.serialize_field(
            "max_inbound_connections",
            &self.network.max_inbound_connections,
        )?;
        st.serialize_field("max_inbound_streams", &self.network.max_inbound_streams)?;
        st.serialize_field("max_gossip_bytes", &self.network.max_gossip_bytes)?;
        st.serialize_field("enable_rpc_auth", &self.rpc_auth.enable)?;
        st.end()
    }
}

impl Default for OnvmConfig {
    fn default() -> Self {
        Self {
            genesis: GenesisConfig { state_root: None },
            network: NetworkConfig {
                min_peers: 1,
                bootnodes: vec![
                    "/ip4/192.168.1.102/tcp/37000/p2p/12D3KooWQUX1oDS8r2v1q27bJ9TwHhuBDy7hJCX6SqgrkVLF1f27"
                        .to_string(),
                ],
                enable_mdns: false,
                require_encryption: true,
                max_inbound_connections: 200,
                max_inbound_streams: 64,
                max_gossip_bytes: 256 * 1024,
            },
            rpc_auth: RpcAuthConfig {
                enable: false,
                project_id: "default-project".into(),
                secret_path: None,
                secondary_secret_path: None,
                skew_ms: 5 * 60 * 1000,
                nonce_ttl_ms: 10 * 60 * 1000,
                nonce_capacity: 10_000,
                allowed_methods: Vec::new(),
                rate_limit_per_minute: 0,
            },
        }
    }
}

impl OnvmConfig {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let content = std::fs::read_to_string(&path)?;
        let value: toml::Value = toml::from_str(&content)?;

        let network = value
            .get("network")
            .cloned()
            .unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));
        let rpc_auth = value
            .get("rpc_auth")
            .cloned()
            .unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));

        let mut cfg = Self::default();
        if let Some(genesis) = value.get("genesis") {
            if let Some(root) = genesis.get("state_root").and_then(|v| v.as_str()) {
                if !root.is_empty() {
                    if let Ok(bytes) = hex::decode(root) {
                        if bytes.len() == 32 {
                            let mut arr = [0u8; 32];
                            arr.copy_from_slice(&bytes);
                            cfg.genesis.state_root = Some(arr);
                        }
                    }
                }
            }
        }
        if let Some(min_peers) = network.get("min_peers").and_then(|v| v.as_integer()) {
            cfg.network.min_peers = min_peers as usize;
        }
        if let Some(bootnodes) = network.get("bootnodes").and_then(|v| v.as_array()) {
            cfg.network.bootnodes = bootnodes
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(mdns) = network.get("enable_mdns").and_then(|v| v.as_bool()) {
            cfg.network.enable_mdns = mdns;
        }
        if let Some(req_enc) = network.get("require_encryption").and_then(|v| v.as_bool()) {
            cfg.network.require_encryption = req_enc;
        }
        if let Some(max_conn) = network
            .get("max_inbound_connections")
            .and_then(|v| v.as_integer())
        {
            cfg.network.max_inbound_connections = max_conn as usize;
        }
        if let Some(max_streams) = network
            .get("max_inbound_streams")
            .and_then(|v| v.as_integer())
        {
            cfg.network.max_inbound_streams = max_streams as usize;
        }
        if let Some(max_gossip) = network.get("max_gossip_bytes").and_then(|v| v.as_integer()) {
            cfg.network.max_gossip_bytes = max_gossip as usize;
        }
        if let Some(enable) = rpc_auth.get("enable_rpc_auth").and_then(|v| v.as_bool()) {
            cfg.rpc_auth.enable = enable;
        }
        if let Some(id) = rpc_auth.get("project_id").and_then(|v| v.as_str()) {
            if !id.is_empty() {
                cfg.rpc_auth.project_id = id.to_string();
            }
        }
        if let Some(secret) = rpc_auth
            .get("secret_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            cfg.rpc_auth.secret_path = Some(secret.to_string());
        }
        if let Some(secret) = rpc_auth
            .get("secondary_secret_path")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            cfg.rpc_auth.secondary_secret_path = Some(secret.to_string());
        }
        if let Some(skew) = rpc_auth.get("skew_ms").and_then(|v| v.as_integer()) {
            cfg.rpc_auth.skew_ms = skew as u64;
        }
        if let Some(ttl) = rpc_auth.get("nonce_ttl_ms").and_then(|v| v.as_integer()) {
            cfg.rpc_auth.nonce_ttl_ms = ttl as u64;
        }
        if let Some(cap) = rpc_auth.get("nonce_capacity").and_then(|v| v.as_integer()) {
            cfg.rpc_auth.nonce_capacity = cap as usize;
        }
        if let Some(methods) = rpc_auth.get("allowed_methods").and_then(|v| v.as_array()) {
            cfg.rpc_auth.allowed_methods = methods
                .iter()
                .filter_map(|m| m.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(rate) = rpc_auth
            .get("rate_limit_per_minute")
            .and_then(|v| v.as_integer())
        {
            cfg.rpc_auth.rate_limit_per_minute = rate as u64;
        }

        Ok(cfg)
    }

    pub fn write_to(path: impl AsRef<Path>) -> Result<()> {
        let cfg = Self::default().to_toml();
        let content = toml::to_string_pretty(&cfg)?;
        fs::write(path, content)?;
        Ok(())
    }
}
