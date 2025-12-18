use super::runtime::{ExecutionConfig, ExecutionEngine};
use crate::storage::StateStore;
use crate::types::ProgramId;
use crate::wasm_runtime::manifest::{AllowedImports, RuntimeConfig};
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use wasmtime::Module;

pub struct SandboxValidator {
    max_fuel_limit: u64,
    max_memory_limit: u64,
    max_timeout_ms: u64,
}

impl Default for SandboxValidator {
    fn default() -> Self {
        Self {
            max_fuel_limit: 500_000_000,
            max_memory_limit: 512 * 1024 * 1024,
            max_timeout_ms: 300_000,
        }
    }
}

impl SandboxValidator {
    pub fn new(max_fuel: u64, max_memory: u64, max_timeout: u64) -> Self {
        Self {
            max_fuel_limit: max_fuel,
            max_memory_limit: max_memory,
            max_timeout_ms: max_timeout,
        }
    }

    pub fn validate_resource_limits(&self, fuel: u64, memory: u64, timeout: u64) -> Result<()> {
        if fuel > self.max_fuel_limit {
            return Err(anyhow!(
                "fuel limit {} exceeds maximum {}",
                fuel,
                self.max_fuel_limit
            ));
        }

        if memory > self.max_memory_limit {
            return Err(anyhow!(
                "memory limit {} exceeds maximum {}",
                memory,
                self.max_memory_limit
            ));
        }

        if timeout > self.max_timeout_ms {
            return Err(anyhow!(
                "timeout {} exceeds maximum {}",
                timeout,
                self.max_timeout_ms
            ));
        }

        Ok(())
    }

    pub fn validate_wasm_module(&self, wasm: &[u8], runtime_config: &RuntimeConfig) -> Result<()> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Disable);

        let engine = wasmtime::Engine::new(&config)?;
        let module = Module::new(&engine, wasm).context("failed to validate WASM module")?;

        self.validate_imports(&module, runtime_config)?;

        self.validate_exports(&module)?;

        Ok(())
    }

    fn validate_imports(&self, module: &Module, runtime_config: &RuntimeConfig) -> Result<()> {
        let imports: Vec<_> = module
            .imports()
            .map(|imp| format!("{}.{}", imp.module(), imp.name()))
            .collect();

        match &runtime_config.allowed_imports {
            AllowedImports::All => Ok(()),
            AllowedImports::None => {
                if !imports.is_empty() {
                    return Err(anyhow!("module has imports but none are allowed"));
                }
                Ok(())
            }
            AllowedImports::Restricted(allowed) => {
                for import in &imports {
                    if !allowed.contains(import) && !import.starts_with("wasi_") {
                        return Err(anyhow!("module imports disallowed function: {}", import));
                    }
                }
                Ok(())
            }
        }
    }

    fn validate_exports(&self, module: &Module) -> Result<()> {
        let has_memory = module.exports().any(|exp| {
            exp.name() == "memory" && matches!(exp.ty(), wasmtime::ExternType::Memory(_))
        });

        if !has_memory {
            return Err(anyhow!("module must export 'memory'"));
        }

        Ok(())
    }
}

pub struct SandboxedExecutor {
    validator: SandboxValidator,
    base_engine: Arc<ExecutionEngine>,
}

impl SandboxedExecutor {
    pub fn new(
        unified_store: Arc<crate::storage::UnifiedStore>,
        state_store: Arc<StateStore>,
        execution_adapter: Arc<crate::wasm_runtime::ExecutionAdapter>,
        validator: SandboxValidator,
    ) -> Result<Self> {
        let config = ExecutionConfig {
            max_fuel: validator.max_fuel_limit,
        };
        let base_engine = Arc::new(ExecutionEngine::new(
            unified_store,
            state_store,
            execution_adapter,
            config,
        )?);

        Ok(Self {
            validator,
            base_engine,
        })
    }

    pub fn validate_program(&self, wasm: &[u8], runtime_config: &RuntimeConfig) -> Result<()> {
        self.validator.validate_wasm_module(wasm, runtime_config)
    }

    pub fn validate_limits(&self, fuel: u64, memory: u64, timeout: u64) -> Result<()> {
        self.validator
            .validate_resource_limits(fuel, memory, timeout)
    }

    pub async fn execute_sandboxed(
        &self,
        program_id: &ProgramId,
        input: &[u8],
        max_fuel: u64,
        timeout_ms: u64,
    ) -> Result<super::runtime::ExecutionOutcome> {
        self.validator
            .validate_resource_limits(max_fuel, 64 * 1024 * 1024, timeout_ms)?;

        let engine = Arc::clone(&self.base_engine);
        let program_id = program_id.clone();
        let input = input.to_vec();

        let timeout_duration = tokio::time::Duration::from_millis(timeout_ms);

        let result = tokio::time::timeout(
            timeout_duration,
            tokio::task::spawn_blocking(move || {
                let config = ExecutionConfig { max_fuel };
                let temp_engine = ExecutionEngine::new(
                    engine.blob_store(),
                    engine.state_store(),
                    engine.program_store(),
                    config,
                )?;
                temp_engine.execute(&program_id, &input)
            }),
        )
        .await;

        match result {
            Ok(Ok(outcome)) => outcome,
            Ok(Err(e)) => Err(anyhow!("execution task error: {e}")),
            Err(_) => Err(anyhow!("execution timed out after {}ms", timeout_ms)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_rejects_excessive_fuel() {
        let validator = SandboxValidator::default();
        let result = validator.validate_resource_limits(1_000_000_000, 64 * 1024 * 1024, 30_000);
        assert!(result.is_err());
    }

    #[test]
    fn test_validator_accepts_valid_limits() {
        let validator = SandboxValidator::default();
        let result = validator.validate_resource_limits(50_000_000, 64 * 1024 * 1024, 30_000);
        assert!(result.is_ok());
    }
}
