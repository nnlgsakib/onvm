use crate::storage::{StateStore, UnifiedStore};
use crate::types::{ObjectId, ProgramId, StateWrite};
use crate::wasm_runtime::host_apis::{
    attach_blob_host_functions, attach_crypto_host_functions, attach_state_host_functions,
    ExecutionContext,
};
use crate::wasm_runtime::ExecutionAdapter;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::WasiCtxBuilder;

#[derive(Clone)]
pub struct ExecutionEngine {
    engine: Engine,
    unified_store: Arc<UnifiedStore>,
    state_store: Arc<StateStore>,
    execution_adapter: Arc<ExecutionAdapter>,
    max_fuel: u64,
    module_cache: Arc<RwLock<HashMap<ObjectId, Module>>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ExecutionOutcome {
    pub program_id: ProgramId,
    pub return_data: Vec<u8>,
    pub fuel_consumed: u64,
    pub state_writes: Vec<StateWrite>,
    pub state_root: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct ExecutionConfig {
    pub max_fuel: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            max_fuel: 50_000_000,
        }
    }
}

enum EntryPoint {
    Multi(wasmtime::TypedFunc<(i32, i32), (i32, i32)>),
    Packed(wasmtime::TypedFunc<(i32, i32), i64>),
    Sret(wasmtime::TypedFunc<(i32, i32, i32), ()>),
}

impl ExecutionEngine {
    pub fn new(
        unified_store: Arc<UnifiedStore>,
        state_store: Arc<StateStore>,
        execution_adapter: Arc<ExecutionAdapter>,
        cfg: ExecutionConfig,
    ) -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.static_memory_guard_size(0);
        config.dynamic_memory_guard_size(0);
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Disable);
        config.wasm_threads(true);
        config.wasm_reference_types(true);
        config.wasm_bulk_memory(true);
        let engine = Engine::new(&config)?;
        Ok(Self {
            engine,
            unified_store,
            state_store,
            execution_adapter,
            max_fuel: cfg.max_fuel,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn config(&self) -> ExecutionConfig {
        ExecutionConfig {
            max_fuel: self.max_fuel,
        }
    }

    pub fn blob_store(&self) -> Arc<UnifiedStore> {
        Arc::clone(&self.unified_store)
    }

    pub fn state_store(&self) -> Arc<StateStore> {
        Arc::clone(&self.state_store)
    }

    pub fn program_store(&self) -> Arc<ExecutionAdapter> {
        Arc::clone(&self.execution_adapter)
    }

    pub fn execute(&self, program_id: &ProgramId, input: &[u8]) -> Result<ExecutionOutcome> {
        let object_id = program_id.to_object_id();

        let wasm = self
            .execution_adapter
            .load_wasm_by_object_id(&object_id)
            .with_context(|| format!("load program {program_id}"))?;

        let entrypoint = self.execution_adapter.get_entrypoint(&object_id)?;

        let module = self.get_or_compile_module(&object_id, &wasm)?;

        let wasi = WasiCtxBuilder::new().inherit_stdio().inherit_env()?.build();
        let ctx = ExecutionContext {
            unified_store: self.unified_store.clone(),
            state_store: self.state_store.clone(),
            program_id: program_id.clone(),
            pending_writes: HashMap::new(),
            wasi,
        };
        let mut store = Store::new(&self.engine, ctx);
        store.set_fuel(self.max_fuel)?;
        let mut linker = Linker::new(&self.engine);
        attach_blob_host_functions(&mut linker)?;
        attach_crypto_host_functions(&mut linker)?;
        attach_state_host_functions(&mut linker)?;
        wasmtime_wasi::add_to_linker(&mut linker, |cx: &mut ExecutionContext| &mut cx.wasi)?;
        let instance = linker.instantiate(&mut store, &module)?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("program missing exported memory"))?;
        let entry_multi =
            instance.get_typed_func::<(i32, i32), (i32, i32)>(&mut store, &entrypoint);
        let entry_packed = instance.get_typed_func::<(i32, i32), i64>(&mut store, &entrypoint);
        let entry_sret = instance.get_typed_func::<(i32, i32, i32), ()>(&mut store, &entrypoint);
        let entry = match (entry_multi, entry_packed, entry_sret) {
            (Ok(f), _, _) => EntryPoint::Multi(f),
            (_, Ok(f), _) => EntryPoint::Packed(f),
            (_, _, Ok(f)) => EntryPoint::Sret(f),
            _ => {
                return Err(anyhow::anyhow!(
                    "program must export {} as (i32,i32)->(i32,i32), (i32,i32)->i64, or (i32,i32,i32)->() with sret",
                    entrypoint
                ))
            }
        };

        let ptr = 0;
        memory.write(&mut store, ptr as usize, input)?;

        let scratch_ptr = ptr + input.len() as i32 + 64;

        let (out_ptr, out_len) = match entry {
            EntryPoint::Multi(f) => f.call(&mut store, (ptr, input.len() as i32))?,
            EntryPoint::Packed(f) => {
                let packed = f.call(&mut store, (ptr, input.len() as i32))?;
                let out_ptr = (packed >> 32) as i32;
                let out_len = packed as i32;
                (out_ptr, out_len)
            }
            EntryPoint::Sret(f) => {
                memory.write(&mut store, scratch_ptr as usize, &[0u8; 8])?;
                f.call(&mut store, (scratch_ptr, ptr, input.len() as i32))?;
                let mut meta_buf = [0u8; 8];
                memory.read(&mut store, scratch_ptr as usize, &mut meta_buf)?;
                let out_ptr = i32::from_le_bytes(meta_buf[0..4].try_into().unwrap());
                let out_len = i32::from_le_bytes(meta_buf[4..8].try_into().unwrap());
                (out_ptr, out_len)
            }
        };
        let mut buf = vec![0u8; out_len as usize];
        memory.read(&mut store, out_ptr as usize, &mut buf)?;
        let remaining = store.get_fuel().unwrap_or(0);
        let consumed = self.max_fuel.saturating_sub(remaining);
        let mut writes: Vec<(Vec<u8>, Vec<u8>)> = store
            .data()
            .pending_writes
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        writes.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in &writes {
            self.state_store
                .set_scoped(&program_id.0, k, v)
                .context("state apply")?;
        }
        let state_root = self
            .state_store
            .root_scoped(&program_id.0)
            .context("state root")?;
        let state_writes = writes
            .into_iter()
            .map(|(key, value)| StateWrite { key, value })
            .collect();
        Ok(ExecutionOutcome {
            program_id: program_id.clone(),
            return_data: buf,
            fuel_consumed: consumed,
            state_writes,
            state_root,
        })
    }

    fn get_or_compile_module(&self, oid: &ObjectId, wasm: &[u8]) -> Result<Module> {
        if let Some(cached) = self.module_cache.read().unwrap().get(oid).cloned() {
            return Ok(cached);
        }
        let module = Module::new(&self.engine, wasm)
            .with_context(|| format!("compile module for object {}", oid))?;
        self.module_cache
            .write()
            .unwrap()
            .insert(*oid, module.clone());
        Ok(module)
    }
}
