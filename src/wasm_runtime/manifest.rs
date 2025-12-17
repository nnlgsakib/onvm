use crate::types::{BlobId, ProgramId};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobManifest {
    pub version: ManifestVersion,
    pub metadata: JobMetadata,
    pub resources: ResourceRequirements,
    pub runtime: RuntimeConfig,
    pub io: IoSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ManifestVersion {
    #[serde(rename = "v1")]
    V1,
}

impl Default for ManifestVersion {
    fn default() -> Self {
        Self::V1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobMetadata {
    pub name: String,
    pub description: Option<String>,
    pub author: Option<String>,
    pub version: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceRequirements {
    pub max_fuel: u64,
    pub max_memory_bytes: u64,
    pub timeout_ms: u64,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            max_fuel: 50_000_000,
            max_memory_bytes: 64 * 1024 * 1024,
            timeout_ms: 30_000,
            capabilities: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Capability {
    #[serde(rename = "state")]
    State,
    #[serde(rename = "blob_read")]
    BlobRead,
    #[serde(rename = "network")]
    Network,
    #[serde(rename = "random")]
    Random,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub allowed_imports: AllowedImports,
    pub signature_required: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            allowed_imports: AllowedImports::Restricted(vec![
                "env.onvm_blob_read".to_string(),
                "env.onvm_state_put".to_string(),
                "env.onvm_state_get".to_string(),
                "env.onvm_state_root".to_string(),
            ]),
            signature_required: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AllowedImports {
    #[serde(rename = "all")]
    All,
    #[serde(rename = "restricted")]
    Restricted(Vec<String>),
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IoSchema {
    #[serde(default)]
    pub input_content_type: ContentType,
    #[serde(default)]
    pub output_content_type: ContentType,
    pub input_schema: Option<String>,
    pub output_schema: Option<String>,
}

impl Default for IoSchema {
    fn default() -> Self {
        Self {
            input_content_type: ContentType::Json,
            output_content_type: ContentType::Json,
            input_schema: None,
            output_schema: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ContentType {
    #[serde(rename = "application/json")]
    Json,
    #[serde(rename = "application/octet-stream")]
    Binary,
    #[serde(rename = "text/plain")]
    Text,
    #[serde(rename = "application/cbor")]
    Cbor,
}

impl Default for ContentType {
    fn default() -> Self {
        Self::Json
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobSubmission {
    pub request_id: String,
    pub program_id: ProgramId,
    pub input: JobInput,
    pub manifest_override: Option<ResourceRequirements>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum JobInput {
    Inline { data: Vec<u8> },
    BlobRef { id: BlobId },
}

impl JobManifest {
    pub fn validate(&self) -> Result<()> {
        if self.metadata.name.is_empty() {
            return Err(anyhow!("manifest name cannot be empty"));
        }

        if self.metadata.version.is_empty() {
            return Err(anyhow!("manifest version cannot be empty"));
        }

        if self.resources.max_fuel == 0 {
            return Err(anyhow!("max_fuel must be > 0"));
        }

        if self.resources.max_memory_bytes < 1024 * 1024 {
            return Err(anyhow!("max_memory_bytes must be >= 1 MiB"));
        }

        if self.resources.timeout_ms == 0 {
            return Err(anyhow!("timeout_ms must be > 0"));
        }

        Ok(())
    }

    pub fn merge_overrides(&self, overrides: &ResourceRequirements) -> ResourceRequirements {
        ResourceRequirements {
            max_fuel: self.resources.max_fuel.min(overrides.max_fuel),
            max_memory_bytes: self
                .resources
                .max_memory_bytes
                .min(overrides.max_memory_bytes),
            timeout_ms: self.resources.timeout_ms.min(overrides.timeout_ms),
            capabilities: self.resources.capabilities.clone(),
        }
    }
}
