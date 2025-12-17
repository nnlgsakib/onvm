use crate::storage::{StateStore, UnifiedStore};
use crate::types::ProgramId;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuelEstimate {
    pub estimated_fuel: u64,
    pub confidence: f64,
    pub based_on_samples: usize,
    pub input_size_bytes: usize,
    pub estimated_execution_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuelProfile {
    pub program_id: ProgramId,
    pub samples: Vec<FuelSample>,
    pub average_fuel_per_byte: f64,
    pub base_fuel_cost: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuelSample {
    pub input_size: usize,
    pub fuel_consumed: u64,
    pub execution_time_ms: u64,
    pub timestamp: u64,
}

pub struct FuelEstimator {
    unified_store: Arc<UnifiedStore>,
    #[allow(dead_code)]
    state_store: Arc<StateStore>,
    profiles: Arc<RwLock<HashMap<ProgramId, FuelProfile>>>,
}

impl FuelEstimator {
    pub fn new(unified_store: Arc<UnifiedStore>, state_store: Arc<StateStore>) -> Self {
        Self {
            unified_store,
            state_store,
            profiles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn estimate_fuel(
        &self,
        program_id: &ProgramId,
        input: &[u8],
    ) -> Result<FuelEstimate> {
        let profiles = self.profiles.read().await;
        
        if let Some(profile) = profiles.get(program_id) {
            let input_size = input.len();
            
            let estimated_fuel = if profile.samples.is_empty() {
                profile.base_fuel_cost + (input_size as f64 * profile.average_fuel_per_byte) as u64
            } else {
                let avg_fuel = profile.samples.iter().map(|s| s.fuel_consumed).sum::<u64>() 
                    / profile.samples.len() as u64;
                let size_ratio = input_size as f64 / 
                    (profile.samples.iter().map(|s| s.input_size).sum::<usize>() as f64 
                        / profile.samples.len() as f64);
                (avg_fuel as f64 * size_ratio) as u64
            };
            
            let avg_time = if !profile.samples.is_empty() {
                profile.samples.iter().map(|s| s.execution_time_ms).sum::<u64>() 
                    / profile.samples.len() as u64
            } else {
                100
            };
            
            let confidence = if profile.samples.len() >= 10 {
                0.95
            } else if profile.samples.len() >= 5 {
                0.80
            } else if profile.samples.len() >= 1 {
                0.60
            } else {
                0.30
            };
            
            Ok(FuelEstimate {
                estimated_fuel,
                confidence,
                based_on_samples: profile.samples.len(),
                input_size_bytes: input_size,
                estimated_execution_time_ms: avg_time,
            })
        } else {
            self.estimate_via_dry_run(program_id, input).await
        }
    }

    async fn estimate_via_dry_run(
        &self,
        program_id: &ProgramId,
        input: &[u8],
    ) -> Result<FuelEstimate> {
        let object_id = program_id.to_object_id();
        
        if !self.unified_store.is_complete(&object_id)? {
            return Err(anyhow!("program not available for estimation"));
        }
        
        let wasm_bytes = self.unified_store.get_object(&object_id)?;
        
        let estimated_fuel = (wasm_bytes.len() as u64 * 100) + (input.len() as u64 * 50) + 50_000;
        
        Ok(FuelEstimate {
            estimated_fuel,
            confidence: 0.30,
            based_on_samples: 0,
            input_size_bytes: input.len(),
            estimated_execution_time_ms: 100,
        })
    }

    pub async fn record_execution(
        &self,
        program_id: &ProgramId,
        input_size: usize,
        fuel_consumed: u64,
        execution_time_ms: u64,
    ) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        
        let profile = profiles.entry(program_id.clone()).or_insert_with(|| {
            FuelProfile {
                program_id: program_id.clone(),
                samples: Vec::new(),
                average_fuel_per_byte: 10.0,
                base_fuel_cost: 10_000,
            }
        });
        
        let sample = FuelSample {
            input_size,
            fuel_consumed,
            execution_time_ms,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };
        
        profile.samples.push(sample);
        
        if profile.samples.len() > 100 {
            profile.samples.remove(0);
        }
        
        if profile.samples.len() >= 3 {
            let total_fuel: u64 = profile.samples.iter().map(|s| s.fuel_consumed).sum();
            let total_size: usize = profile.samples.iter().map(|s| s.input_size).sum();
            
            if total_size > 0 {
                profile.average_fuel_per_byte = total_fuel as f64 / total_size as f64;
            }
            
            let min_fuel = profile.samples.iter().map(|s| s.fuel_consumed).min().unwrap_or(0);
            profile.base_fuel_cost = min_fuel;
        }
        
        Ok(())
    }

    pub async fn get_profile(&self, program_id: &ProgramId) -> Option<FuelProfile> {
        let profiles = self.profiles.read().await;
        profiles.get(program_id).cloned()
    }

    pub async fn clear_profile(&self, program_id: &ProgramId) -> Result<()> {
        let mut profiles = self.profiles.write().await;
        profiles.remove(program_id);
        Ok(())
    }
}