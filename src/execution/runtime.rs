use crate::execution::ProgramStore;
use crate::storage::{BlobStore, StateStore};
use crate::types::{BlobId, ProgramId, StateWrite};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::WasiCtxBuilder;

#[derive(Clone)]
pub struct ExecutionEngine {
    engine: Engine,
    blob_store: Arc<BlobStore>,
    state_store: Arc<StateStore>,
    programs: Arc<ProgramStore>,
    max_fuel: u64,
    module_cache: Arc<RwLock<HashMap<ProgramId, Module>>>,
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

struct ExecutionContext {
    blob_store: Arc<BlobStore>,
    state_store: Arc<StateStore>,
    program_id: ProgramId,
    pending_writes: HashMap<Vec<u8>, Vec<u8>>,
    wasi: wasmtime_wasi::WasiCtx,
}

impl ExecutionEngine {
    pub fn new(
        blob_store: Arc<BlobStore>,
        state_store: Arc<StateStore>,
        programs: Arc<ProgramStore>,
        cfg: ExecutionConfig,
    ) -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.static_memory_guard_size(0);
        config.dynamic_memory_guard_size(0);
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Disable);
        let engine = Engine::new(&config)?;
        Ok(Self {
            engine,
            blob_store,
            state_store,
            programs,
            max_fuel: cfg.max_fuel,
            module_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn execute(&self, program_id: &ProgramId, input: &[u8]) -> Result<ExecutionOutcome> {
        let meta = self
            .programs
            .metadata(program_id)?
            .ok_or_else(|| anyhow::anyhow!("program metadata missing"))?;
        let wasm = self
            .programs
            .load(program_id)
            .with_context(|| format!("load program {program_id}"))?;
        let module = self.get_or_compile_module(program_id, &wasm)?;

        let wasi = WasiCtxBuilder::new().build();
        let ctx = ExecutionContext {
            blob_store: self.blob_store.clone(),
            state_store: self.state_store.clone(),
            program_id: program_id.clone(),
            pending_writes: HashMap::new(),
            wasi,
        };
        let mut store = Store::new(&self.engine, ctx);
        store.set_fuel(self.max_fuel)?;
        let mut linker = Linker::new(&self.engine);
        attach_blob_host_functions(&mut linker)?;
        attach_state_host_functions(&mut linker)?;
        // Attach WASI to broaden compatibility; still sandboxed (no FS by default).
        wasmtime_wasi::add_to_linker(&mut linker, |cx: &mut ExecutionContext| &mut cx.wasi)?;
        let instance = linker.instantiate(&mut store, &module)?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("program missing exported memory"))?;
        let entry_multi =
            instance.get_typed_func::<(i32, i32), (i32, i32)>(&mut store, &meta.entrypoint);
        let entry_packed = instance.get_typed_func::<(i32, i32), i64>(&mut store, &meta.entrypoint);
        let entry_sret =
            instance.get_typed_func::<(i32, i32, i32), ()>(&mut store, &meta.entrypoint);
        let entry = match (entry_multi, entry_packed, entry_sret) {
            (Ok(f), _, _) => EntryPoint::Multi(f),
            (_, Ok(f), _) => EntryPoint::Packed(f),
            (_, _, Ok(f)) => EntryPoint::Sret(f),
            _ => {
                return Err(anyhow::anyhow!(
                    "program must export {} as (i32,i32)->(i32,i32), (i32,i32)->i64, or (i32,i32,i32)->() with sret",
                    meta.entrypoint
                ))
            }
        };

        let ptr = 0;
        memory.write(&mut store, ptr as usize, input)?;

        // scratch space for sret / output metadata placed after input
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
                // write zeroes to scratch, call, then read two i32 results back
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
        // Apply pending writes deterministically by key order.
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

    fn get_or_compile_module(&self, pid: &ProgramId, wasm: &[u8]) -> Result<Module> {
        if let Some(cached) = self.module_cache.read().unwrap().get(pid).cloned() {
            return Ok(cached);
        }
        let module = Module::new(&self.engine, wasm)
            .with_context(|| format!("compile module for program {}", pid))?;
        self.module_cache
            .write()
            .unwrap()
            .insert(pid.clone(), module.clone());
        Ok(module)
    }
}

fn attach_blob_host_functions(linker: &mut Linker<ExecutionContext>) -> Result<()> {
    linker.func_wrap(
        "env",
        "onvm_blob_read",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>,
         id_ptr: i32,
         id_len: i32,
         out_ptr: i32,
         out_capacity: i32|
         -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let mut id_buf = vec![0u8; id_len as usize];
            if memory.read(&caller, id_ptr as usize, &mut id_buf).is_err() {
                return -2;
            }
            let id_hex = match String::from_utf8(id_buf) {
                Ok(s) => s,
                Err(_) => return -3,
            };
            let Ok(id_bytes) = hex::decode(id_hex.trim()) else {
                return -4;
            };
            if id_bytes.len() != 32 {
                return -5;
            }
            let mut blob_id = [0u8; 32];
            blob_id.copy_from_slice(&id_bytes);
            let blob_id = BlobId(blob_id);
            let data = match caller.data().blob_store.get(&blob_id) {
                Ok(d) => d,
                Err(_) => return -6,
            };
            if data.len() as i32 > out_capacity {
                return -7;
            }
            if memory.write(&mut caller, out_ptr as usize, &data).is_err() {
                return -8;
            }
            data.len() as i32
        },
    )?;
    Ok(())
}

fn attach_state_host_functions(linker: &mut Linker<ExecutionContext>) -> Result<()> {
    // onvm_state_put(key_ptr, key_len, val_ptr, val_len) -> i32 (0 ok, negative err)
    linker.func_wrap(
        "env",
        "onvm_state_put",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>,
         key_ptr: i32,
         key_len: i32,
         val_ptr: i32,
         val_len: i32|
         -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let mut key_buf = vec![0u8; key_len as usize];
            if memory.read(&caller, key_ptr as usize, &mut key_buf).is_err() {
                return -2;
            }
            let mut val_buf = vec![0u8; val_len as usize];
            if memory
                .read(&caller, val_ptr as usize, &mut val_buf)
                .is_err()
            {
                return -3;
            }
            caller
                .data_mut()
                .pending_writes
                .insert(key_buf, val_buf);
            0
        },
    )?;

    // onvm_state_get(key_ptr, key_len, out_ptr, out_cap) -> i32 (len on success, 0 if missing, negative if err or cap too small: -needed)
    linker.func_wrap(
        "env",
        "onvm_state_get",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>,
         key_ptr: i32,
         key_len: i32,
         out_ptr: i32,
         out_cap: i32|
         -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let mut key_buf = vec![0u8; key_len as usize];
            if memory.read(&caller, key_ptr as usize, &mut key_buf).is_err() {
                return -2;
            }
            // prefer pending writes overlay
            if let Some(v) = caller
                .data()
                .pending_writes
                .get(&key_buf)
                .cloned()
            {
                let len = v.len() as i32;
                if len > out_cap {
                    return -len;
                }
                if memory.write(&mut caller, out_ptr as usize, &v).is_err() {
                    return -4;
                }
                return len;
            }
            let ns = caller.data().program_id.0;
            let val = match caller.data().state_store.get_scoped(&ns, &key_buf) {
                Ok(Some(v)) => v,
                Ok(None) => return 0,
                Err(_) => return -3,
            };
            let len = val.len() as i32;
            if len > out_cap {
                return -len;
            }
            if memory
                .write(&mut caller, out_ptr as usize, &val)
                .is_err()
            {
                return -4;
            }
            len
        },
    )?;

    // onvm_state_root(out_ptr) -> i32 (0 ok, negative err). Writes 32 bytes.
    linker.func_wrap(
        "env",
        "onvm_state_root",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>, out_ptr: i32| -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let ns = caller.data().program_id.0;
            // include pending writes not yet applied by reading overlay into a temp state root
            let pairs: Vec<_> = caller
                .data()
                .pending_writes
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let root = if pairs.is_empty() {
                match caller.data().state_store.root_scoped(&ns) {
                    Ok(r) => r,
                    Err(_) => return -2,
                }
            } else {
                let mut current = caller.data().state_store.root_scoped(&ns).unwrap_or([0u8; 32]);
                // simplistic: fold hashes of pending writes into root
                for (k, v) in pairs {
                    current = crate::crypto::hashing::hash_bytes(
                        [current.as_slice(), k.as_slice(), v.as_slice()].concat().as_slice(),
                    );
                }
                current
            };
            if memory
                .write(&mut caller, out_ptr as usize, &root)
                .is_err()
            {
                return -3;
            }
            0
        },
    )?;
    Ok(())
}
