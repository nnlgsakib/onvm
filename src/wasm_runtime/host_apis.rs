use crate::crypto::hashing::hash_bytes;
use crate::storage::{StateStore, UnifiedStore};
use crate::types::{ObjectId, ProgramId};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ExecutionContext {
    pub unified_store: Arc<UnifiedStore>,
    pub state_store: Arc<StateStore>,
    pub program_id: ProgramId,
    pub pending_writes: HashMap<Vec<u8>, Vec<u8>>,
    pub wasi: wasmtime_wasi::WasiCtx,
}

pub fn attach_blob_host_functions(linker: &mut wasmtime::Linker<ExecutionContext>) -> Result<()> {
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
            let mut object_id_arr = [0u8; 32];
            object_id_arr.copy_from_slice(&id_bytes);
            let object_id = ObjectId(object_id_arr);
            let data = match caller.data().unified_store.get_object(&object_id) {
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

pub fn attach_state_host_functions(linker: &mut wasmtime::Linker<ExecutionContext>) -> Result<()> {
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
            if memory
                .read(&caller, key_ptr as usize, &mut key_buf)
                .is_err()
            {
                return -2;
            }
            let mut val_buf = vec![0u8; val_len as usize];
            if memory
                .read(&caller, val_ptr as usize, &mut val_buf)
                .is_err()
            {
                return -3;
            }
            caller.data_mut().pending_writes.insert(key_buf, val_buf);
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
            if memory
                .read(&caller, key_ptr as usize, &mut key_buf)
                .is_err()
            {
                return -2;
            }
            // prefer pending writes overlay
            if let Some(v) = caller.data().pending_writes.get(&key_buf).cloned() {
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
            if memory.write(&mut caller, out_ptr as usize, &val).is_err() {
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
                let mut current = caller
                    .data()
                    .state_store
                    .root_scoped(&ns)
                    .unwrap_or([0u8; 32]);
                // simplistic: fold hashes of pending writes into root
                for (k, v) in pairs {
                    current = hash_bytes(
                        [current.as_slice(), k.as_slice(), v.as_slice()]
                            .concat()
                            .as_slice(),
                    );
                }
                current
            };
            if memory.write(&mut caller, out_ptr as usize, &root).is_err() {
                return -3;
            }
            0
        },
    )?;
    Ok(())
}
