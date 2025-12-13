//! Centralized runtime configuration for ONVM nodes.
//!
//! This holds genesis/static chain parameters, block production parameters,
//! and networking defaults so components share a single source of truth.

use anyhow::Result;
use std::fs;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct GenesisConfig {
    /// Optional precomputed global state root; if absent, an empty root is assumed.
    pub state_root: Option<[u8; 32]>,
}

#[derive(Clone, Debug)]
pub struct BlockConfig {
    /// Max operations per block before sealing.
    pub max_batch: usize,
    /// Target slot/producing interval for blocks.
    pub slot_duration: Duration,
    /// Minimum operations required to emit a block (prevents empty/heartbeat blocks).
    pub min_ops: usize,
}

#[derive(Clone, Debug)]
pub struct NetworkConfig {
    /// Minimum number of peers (excluding self) required to produce blocks.
    pub min_peers: usize,
    /// Default bootnodes to attempt dialing on startup.
    pub bootnodes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct OnvmConfig {
    pub genesis: GenesisConfig,
    pub block: BlockConfig,
    pub network: NetworkConfig,
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

        let mut block = toml::map::Map::new();
        block.insert(
            "max_batch".into(),
            toml::Value::Integer(self.block.max_batch as i64),
        );
        block.insert(
            "slot_ms".into(),
            toml::Value::Integer(self.block.slot_duration.as_millis() as i64),
        );
        block.insert(
            "min_ops".into(),
            toml::Value::Integer(self.block.min_ops as i64),
        );
        root.insert("block".into(), toml::Value::Table(block));

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
        root.insert("network".into(), toml::Value::Table(network));

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
        st.serialize_field("block_max_batch", &self.block.max_batch)?;
        st.serialize_field("block_slot_ms", &self.block.slot_duration.as_millis())?;
        st.serialize_field("block_min_ops", &self.block.min_ops)?;
        st.serialize_field("min_peers", &self.network.min_peers)?;
        st.end()
    }
}

impl Default for OnvmConfig {
    fn default() -> Self {
        Self {
            genesis: GenesisConfig { state_root: None },
            block: BlockConfig {
                max_batch: 512,
                slot_duration: Duration::from_millis(500),
                min_ops: 1,
            },
            network: NetworkConfig {
                min_peers: 1,
                bootnodes: vec![
                    "/ip4/192.168.1.102/tcp/37000/12D3KooWQUX1oDS8r2v1q27bJ9TwHhuBDy7hJCX6SqgrkVLF1f27".to_string(),
                ],
            },
        }
    }
}

impl OnvmConfig {
    pub fn write_to(path: impl AsRef<Path>) -> Result<()> {
        let cfg = Self::default().to_toml();
        let content = toml::to_string_pretty(&cfg)?;
        fs::write(path, content)?;
        Ok(())
    }
}
