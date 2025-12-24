use crate::storage::{StateStore, UnifiedStore};
use crate::types::{ObjectId, ProgramId};
use anyhow::Result;
use rand::rngs::OsRng;
use rand::RngCore;
use secp256k1::ecdsa::{RecoverableSignature, RecoveryId, Signature};
use secp256k1::{Message, PublicKey, Secp256k1};
use sha2::Digest;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_keccak::Hasher;

pub struct ExecutionContext {
    pub unified_store: Arc<UnifiedStore>,
    pub state_store: Arc<StateStore>,
    pub program_id: ProgramId,
    pub pending_writes: HashMap<Vec<u8>, Option<Vec<u8>>>,
    pub wasi: wasmtime_wasi::WasiCtx,
}

pub fn attach_blob_host_functions(linker: &mut wasmtime::Linker<ExecutionContext>) -> Result<()> {
    linker.func_wrap(
        "env",
        "onvm_blob_exists",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>, id_ptr: i32, id_len: i32| -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let object_id = match read_object_id(&memory, &caller, id_ptr, id_len) {
                Ok(oid) => oid,
                Err(code) => return code,
            };
            // Treat a blob as existing if we have a manifest; completeness may depend on sync.
            match caller
                .data()
                .unified_store
                .get_manifest_by_object(&object_id)
            {
                Ok(Some(_)) => 1,
                Ok(None) => 0,
                Err(_) => -6,
            }
        },
    )?;

    linker.func_wrap(
        "env",
        "onvm_blob_len",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>, id_ptr: i32, id_len: i32| -> i64 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let object_id = match read_object_id(&memory, &caller, id_ptr, id_len) {
                Ok(oid) => oid,
                Err(code) => return code as i64,
            };
            if let Ok(Some(meta)) = caller.data().unified_store.get_object_metadata(&object_id) {
                return meta.total_size as i64;
            }
            // Fallback to manifest if metadata not yet cached.
            match caller
                .data()
                .unified_store
                .get_manifest_by_object(&object_id)
            {
                Ok(Some(m)) => m.total_size() as i64,
                Ok(None) => -1,
                Err(_) => -2,
            }
        },
    )?;

    linker.func_wrap(
        "env",
        "onvm_blob_hash",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>,
         id_ptr: i32,
         id_len: i32,
         out_ptr: i32|
         -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let object_id = match read_object_id(&memory, &caller, id_ptr, id_len) {
                Ok(oid) => oid,
                Err(code) => return code,
            };
            let manifest = match caller
                .data()
                .unified_store
                .get_manifest_by_object(&object_id)
            {
                Ok(Some(m)) => m,
                Ok(None) => return -2,
                Err(_) => return -3,
            };
            if memory
                .write(&mut caller, out_ptr as usize, &manifest.content_hash)
                .is_err()
            {
                return -4;
            }
            0
        },
    )?;

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
    linker.func_wrap(
        "env",
        "onvm_state_exists",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>, key_ptr: i32, key_len: i32| -> i32 {
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
            if let Some(entry) = caller.data().pending_writes.get(&key_buf) {
                return if entry.is_some() { 1 } else { 0 };
            }
            let ns = caller.data().program_id.0;
            match caller.data().state_store.get_scoped(&ns, &key_buf) {
                Ok(Some(_)) => 1,
                Ok(None) => 0,
                Err(_) => -3,
            }
        },
    )?;

    linker.func_wrap(
        "env",
        "onvm_state_len",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>, key_ptr: i32, key_len: i32| -> i64 {
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
            if let Some(entry) = caller.data().pending_writes.get(&key_buf) {
                return match entry {
                    Some(v) => v.len() as i64,
                    None => -1,
                };
            }
            let ns = caller.data().program_id.0;
            match caller.data().state_store.get_scoped(&ns, &key_buf) {
                Ok(Some(v)) => v.len() as i64,
                Ok(None) => -1,
                Err(_) => -3,
            }
        },
    )?;

    linker.func_wrap(
        "env",
        "onvm_state_del",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>, key_ptr: i32, key_len: i32| -> i32 {
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
            caller.data_mut().pending_writes.insert(key_buf, None);
            0
        },
    )?;

    linker.func_wrap(
        "env",
        "onvm_state_list",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>,
         prefix_ptr: i32,
         prefix_len: i32,
         out_ptr: i32,
         out_cap: i32|
         -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let mut prefix = vec![0u8; prefix_len as usize];
            if memory
                .read(&caller, prefix_ptr as usize, &mut prefix)
                .is_err()
            {
                return -2;
            }
            let ns = caller.data().program_id.0;

            let mut keys: HashMap<Vec<u8>, ()> = HashMap::new();
            for (k, entry) in &caller.data().pending_writes {
                if entry.is_some() && k.starts_with(&prefix) {
                    keys.insert(k.clone(), ());
                }
            }
            let pairs = match caller.data().state_store.get_all_scoped(&ns) {
                Ok(p) => p,
                Err(_) => return -3,
            };
            for (k, _) in pairs {
                if k.starts_with(&prefix) {
                    if caller
                        .data()
                        .pending_writes
                        .get(&k)
                        .map(|v| v.is_none())
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    keys.entry(k).or_insert(());
                }
            }

            let mut key_list: Vec<Vec<u8>> = keys.into_iter().map(|(k, _)| k).collect();
            key_list.sort();

            let total_bytes: usize = key_list.iter().map(|k| 4 + k.len()).sum();
            if total_bytes > out_cap as usize {
                return -(total_bytes as i32);
            }

            let mut cursor = out_ptr as usize;
            for key in key_list {
                let len = key.len() as u32;
                if memory
                    .write(&mut caller, cursor, &len.to_le_bytes())
                    .is_err()
                {
                    return -4;
                }
                cursor += 4;
                if memory.write(&mut caller, cursor, &key).is_err() {
                    return -5;
                }
                cursor += key.len();
            }
            total_bytes as i32
        },
    )?;

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
            caller
                .data_mut()
                .pending_writes
                .insert(key_buf, Some(val_buf));
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
            if let Some(entry) = caller.data().pending_writes.get(&key_buf).cloned() {
                match entry {
                    Some(v) => {
                        let len = v.len() as i32;
                        if len > out_cap {
                            return -len;
                        }
                        if memory.write(&mut caller, out_ptr as usize, &v).is_err() {
                            return -4;
                        }
                        return len;
                    }
                    None => return 0,
                }
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
            let pairs = match caller.data().state_store.get_all_scoped(&ns) {
                Ok(p) => p,
                Err(_) => return -2,
            };
            let mut map: HashMap<Vec<u8>, Vec<u8>> = pairs.into_iter().collect();
            for (k, entry) in &caller.data().pending_writes {
                match entry {
                    Some(v) => {
                        map.insert(k.clone(), v.clone());
                    }
                    None => {
                        map.remove(k);
                    }
                }
            }
            let merged: Vec<(Vec<u8>, Vec<u8>)> = map.into_iter().collect();
            let root = crate::merkle::sparse_merkle_root(&merged);
            if memory.write(&mut caller, out_ptr as usize, &root).is_err() {
                return -3;
            }
            0
        },
    )?;

    linker.func_wrap(
        "env",
        "onvm_log",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>,
         level: i32,
         msg_ptr: i32,
         msg_len: i32| {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let mut buf = vec![0u8; msg_len as usize];
            if memory.read(&caller, msg_ptr as usize, &mut buf).is_err() {
                return -2;
            }
            let msg = match String::from_utf8(buf) {
                Ok(m) => m,
                Err(_) => return -3,
            };
            match level {
                0 => tracing::trace!(target: "wasm", "{}", msg),
                1 => tracing::debug!(target: "wasm", "{}", msg),
                2 => tracing::info!(target: "wasm", "{}", msg),
                3 => tracing::warn!(target: "wasm", "{}", msg),
                _ => return -4,
            }
            0
        },
    )?;

    linker.func_wrap("env", "onvm_now_ms", || -> i64 {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(dur) => dur.as_millis() as i64,
            Err(_) => -1,
        }
    })?;

    linker.func_wrap(
        "env",
        "onvm_random_bytes",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>, out_ptr: i32, out_len: i32| -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            if out_len < 0 {
                return -2;
            }
            let mut buf = vec![0u8; out_len as usize];
            OsRng.fill_bytes(&mut buf);
            if memory.write(&mut caller, out_ptr as usize, &buf).is_err() {
                return -3;
            }
            0
        },
    )?;

    Ok(())
}

pub fn attach_crypto_host_functions(linker: &mut wasmtime::Linker<ExecutionContext>) -> Result<()> {
    linker.func_wrap(
        "env",
        "onvm_crypto_hash",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>,
         algo: i32,
         in_ptr: i32,
         in_len: i32,
         out_ptr: i32,
         out_cap: i32|
         -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let mut input = vec![0u8; in_len as usize];
            if memory.read(&caller, in_ptr as usize, &mut input).is_err() {
                return -2;
            }
            let digest = match algo {
                1 => {
                    let mut hasher = sha2::Sha256::new();
                    hasher.update(&input);
                    hasher.finalize().to_vec()
                }
                2 => {
                    let mut hasher = tiny_keccak::Keccak::v256();
                    hasher.update(&input);
                    let mut out = [0u8; 32];
                    hasher.finalize(&mut out);
                    out.to_vec()
                }
                _ => return -3,
            };
            if digest.len() as i32 > out_cap {
                return -(digest.len() as i32);
            }
            if memory
                .write(&mut caller, out_ptr as usize, &digest)
                .is_err()
            {
                return -4;
            }
            digest.len() as i32
        },
    )?;

    linker.func_wrap(
        "env",
        "onvm_crypto_secp256k1_verify",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>,
         msg_ptr: i32,
         sig_ptr: i32,
         pubkey_ptr: i32|
         -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let mut msg = [0u8; 32];
            if memory.read(&caller, msg_ptr as usize, &mut msg).is_err() {
                return -2;
            }
            let mut sig_bytes = [0u8; 65];
            if memory
                .read(&caller, sig_ptr as usize, &mut sig_bytes)
                .is_err()
            {
                return -3;
            }
            let mut pk_bytes = [0u8; 64];
            if memory
                .read(&caller, pubkey_ptr as usize, &mut pk_bytes)
                .is_err()
            {
                return -4;
            }

            let secp = Secp256k1::verification_only();

            let message = match Message::from_digest_slice(&msg) {
                Ok(m) => m,
                Err(_) => return -5,
            };

            let signature = match Signature::from_compact(&sig_bytes[..64]) {
                Ok(s) => s,
                Err(_) => return -6,
            };

            let mut uncompressed = Vec::with_capacity(65);
            uncompressed.push(0x04);
            uncompressed.extend_from_slice(&pk_bytes);
            let pubkey = match PublicKey::from_slice(&uncompressed) {
                Ok(pk) => pk,
                Err(_) => return -7,
            };

            match secp.verify_ecdsa(&message, &signature, &pubkey) {
                Ok(_) => 0,
                Err(_) => 1,
            }
        },
    )?;

    linker.func_wrap(
        "env",
        "onvm_crypto_secp256k1_recover",
        |mut caller: wasmtime::Caller<'_, ExecutionContext>,
         msg_ptr: i32,
         sig_ptr: i32,
         out_pubkey_ptr: i32|
         -> i32 {
            let memory = match caller.get_export("memory") {
                Some(wasmtime::Extern::Memory(m)) => m,
                _ => return -1,
            };
            let mut msg = [0u8; 32];
            if memory.read(&caller, msg_ptr as usize, &mut msg).is_err() {
                return -2;
            }
            let mut sig_bytes = [0u8; 65];
            if memory
                .read(&caller, sig_ptr as usize, &mut sig_bytes)
                .is_err()
            {
                return -3;
            }

            let recovery_id = match RecoveryId::from_i32((sig_bytes[64] % 4) as i32) {
                Ok(id) => id,
                Err(_) => return -4,
            };
            let rec_sig = match RecoverableSignature::from_compact(&sig_bytes[..64], recovery_id) {
                Ok(s) => s,
                Err(_) => return -5,
            };
            let message = match Message::from_digest_slice(&msg) {
                Ok(m) => m,
                Err(_) => return -6,
            };

            let secp = Secp256k1::verification_only();
            let pubkey = match secp.recover_ecdsa(&message, &rec_sig) {
                Ok(pk) => pk,
                Err(_) => return -7,
            };
            let uncompressed = pubkey.serialize_uncompressed();
            if uncompressed.len() != 65 {
                return -8;
            }
            if memory
                .write(&mut caller, out_pubkey_ptr as usize, &uncompressed[1..])
                .is_err()
            {
                return -9;
            }
            0
        },
    )?;

    Ok(())
}

fn read_object_id(
    memory: &wasmtime::Memory,
    caller: &wasmtime::Caller<'_, ExecutionContext>,
    id_ptr: i32,
    id_len: i32,
) -> Result<ObjectId, i32> {
    let mut id_buf = vec![0u8; id_len as usize];
    if memory.read(caller, id_ptr as usize, &mut id_buf).is_err() {
        return Err(-2);
    }

    // Accept either raw 32-byte object IDs or 64-char hex strings.
    let id_bytes = if id_buf.len() == 32 {
        id_buf
    } else {
        let id_hex = String::from_utf8(id_buf).map_err(|_| -3)?;
        let decoded = hex::decode(id_hex.trim()).map_err(|_| -4)?;
        decoded
    };

    if id_bytes.len() != 32 {
        return Err(-5);
    }
    let mut object_id_arr = [0u8; 32];
    object_id_arr.copy_from_slice(&id_bytes);
    Ok(ObjectId(object_id_arr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::UnifiedStore;
    use crate::types::{NodeId, ObjectType, ProgramId};
    use anyhow::Result;
    use sled::Config;
    use wasmtime::{Engine, Instance, Linker, Memory, Module, Store};
    use wasmtime_wasi::WasiCtxBuilder;

    const WASM: &str = r#"
    (module
      (import "env" "onvm_blob_exists" (func $blob_exists (param i32 i32) (result i32)))
      (import "env" "onvm_blob_len" (func $blob_len (param i32 i32) (result i64)))
      (import "env" "onvm_blob_hash" (func $blob_hash (param i32 i32 i32) (result i32)))
      (import "env" "onvm_blob_read" (func $blob_read (param i32 i32 i32 i32) (result i32)))
      (import "env" "onvm_state_exists" (func $state_exists (param i32 i32) (result i32)))
      (import "env" "onvm_state_len" (func $state_len (param i32 i32) (result i64)))
      (import "env" "onvm_state_put" (func $state_put (param i32 i32 i32 i32) (result i32)))
      (import "env" "onvm_state_get" (func $state_get (param i32 i32 i32 i32) (result i32)))
      (import "env" "onvm_state_del" (func $state_del (param i32 i32) (result i32)))
      (import "env" "onvm_state_list" (func $state_list (param i32 i32 i32 i32) (result i32)))
      (import "env" "onvm_state_root" (func $state_root (param i32) (result i32)))
      (import "env" "onvm_crypto_hash" (func $crypto_hash (param i32 i32 i32 i32 i32) (result i32)))
      (import "env" "onvm_crypto_secp256k1_verify" (func $crypto_verify (param i32 i32 i32) (result i32)))
      (import "env" "onvm_crypto_secp256k1_recover" (func $crypto_recover (param i32 i32 i32) (result i32)))
      (memory (export "memory") 4)
      (func (export "blob_exists") (param i32 i32) (result i32) (call $blob_exists (local.get 0) (local.get 1)))
      (func (export "blob_len") (param i32 i32) (result i64) (call $blob_len (local.get 0) (local.get 1)))
      (func (export "blob_hash") (param i32 i32 i32) (result i32) (call $blob_hash (local.get 0) (local.get 1) (local.get 2)))
      (func (export "blob_read") (param i32 i32 i32 i32) (result i32) (call $blob_read (local.get 0) (local.get 1) (local.get 2) (local.get 3)))
      (func (export "state_exists") (param i32 i32) (result i32) (call $state_exists (local.get 0) (local.get 1)))
      (func (export "state_len") (param i32 i32) (result i64) (call $state_len (local.get 0) (local.get 1)))
      (func (export "state_put") (param i32 i32 i32 i32) (result i32) (call $state_put (local.get 0) (local.get 1) (local.get 2) (local.get 3)))
      (func (export "state_get") (param i32 i32 i32 i32) (result i32) (call $state_get (local.get 0) (local.get 1) (local.get 2) (local.get 3)))
      (func (export "state_del") (param i32 i32) (result i32) (call $state_del (local.get 0) (local.get 1)))
      (func (export "state_list") (param i32 i32 i32 i32) (result i32) (call $state_list (local.get 0) (local.get 1) (local.get 2) (local.get 3)))
      (func (export "state_root") (param i32) (result i32) (call $state_root (local.get 0)))
      (func (export "crypto_hash") (param i32 i32 i32 i32 i32) (result i32) (call $crypto_hash (local.get 0) (local.get 1) (local.get 2) (local.get 3) (local.get 4)))
      (func (export "crypto_verify") (param i32 i32 i32) (result i32) (call $crypto_verify (local.get 0) (local.get 1) (local.get 2)))
      (func (export "crypto_recover") (param i32 i32 i32) (result i32) (call $crypto_recover (local.get 0) (local.get 1) (local.get 2)))
    )
    "#;

    fn build_store_with_instance() -> Result<(
        Store<ExecutionContext>,
        Instance,
        Memory,
        Arc<UnifiedStore>,
        Arc<StateStore>,
    )> {
        let db = Config::new().temporary(true).open()?;
        let unified = Arc::new(UnifiedStore::new(db.clone())?);
        let state = Arc::new(StateStore::new(db, "state_store")?);
        let engine = Engine::default();
        let mut linker = Linker::new(&engine);
        attach_blob_host_functions(&mut linker)?;
        attach_state_host_functions(&mut linker)?;
        attach_crypto_host_functions(&mut linker)?;
        wasmtime_wasi::add_to_linker(&mut linker, |cx: &mut ExecutionContext| &mut cx.wasi)?;
        let module = Module::new(&engine, WASM)?;
        let ctx = ExecutionContext {
            unified_store: unified.clone(),
            state_store: state.clone(),
            program_id: ProgramId([1u8; 32]),
            pending_writes: HashMap::new(),
            wasi: WasiCtxBuilder::new().build(),
        };
        let mut store = Store::new(&engine, ctx);
        let instance = linker.instantiate(&mut store, &module)?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("memory export");
        Ok((store, instance, memory, unified, state))
    }

    #[test]
    fn blob_host_apis() -> Result<()> {
        let (mut store, instance, memory, unified, _) = build_store_with_instance()?;
        let data = b"hello blob";
        let publisher = NodeId::new(&[9u8; 32]);
        let object =
            unified.put_object(data, ObjectType::blob_with_random_salt(None), publisher)?;
        let hex_id = hex::encode(object.id.0);
        memory.write(&mut store, 0, hex_id.as_bytes())?;

        let exists = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "blob_exists")?
            .call(&mut store, (0, hex_id.len() as i32))?;
        assert_eq!(exists, 1);

        let len = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, "blob_len")?
            .call(&mut store, (0, hex_id.len() as i32))?;
        assert_eq!(len as usize, data.len());

        let hash_status = instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut store, "blob_hash")?
            .call(&mut store, (0, hex_id.len() as i32, 256))?;
        assert_eq!(hash_status, 0);
        let mut hash_out = [0u8; 32];
        memory.read(&store, 256, &mut hash_out)?;
        let manifest = unified
            .get_manifest_by_object(&object.id)?
            .expect("manifest present");
        assert_eq!(hash_out, manifest.content_hash);

        let read_len = instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(&mut store, "blob_read")?
            .call(&mut store, (0, hex_id.len() as i32, 512, 1024))?;
        assert_eq!(read_len as usize, data.len());
        let mut buf = vec![0u8; data.len()];
        memory.read(&store, 512, &mut buf)?;
        assert_eq!(buf, data);
        Ok(())
    }

    #[test]
    fn state_host_apis() -> Result<()> {
        let (mut store, instance, memory, _, state) = build_store_with_instance()?;
        let key = b"foo";
        let value = b"barbaz";
        memory.write(&mut store, 0, key)?;
        memory.write(&mut store, 64, value)?;

        let put_res = instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(&mut store, "state_put")?
            .call(&mut store, (0, key.len() as i32, 64, value.len() as i32))?;
        assert_eq!(put_res, 0);

        let exists = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "state_exists")?
            .call(&mut store, (0, key.len() as i32))?;
        assert_eq!(exists, 1);

        let len = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, "state_len")?
            .call(&mut store, (0, key.len() as i32))?;
        assert_eq!(len as usize, value.len());

        let get_len = instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(&mut store, "state_get")?
            .call(&mut store, (0, key.len() as i32, 128, 64))?;
        assert_eq!(get_len as usize, value.len());
        let mut out = vec![0u8; value.len()];
        memory.read(&store, 128, &mut out)?;
        assert_eq!(out, value);

        // list
        let list_bytes = instance
            .get_typed_func::<(i32, i32, i32, i32), i32>(&mut store, "state_list")?
            .call(&mut store, (0, 0, 256, 128))?;
        assert!(list_bytes > 0);

        // delete and ensure missing
        let del = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "state_del")?
            .call(&mut store, (0, key.len() as i32))?;
        assert_eq!(del, 0);
        let exists_after = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "state_exists")?
            .call(&mut store, (0, key.len() as i32))?;
        assert_eq!(exists_after, 0);

        // state_root should still succeed
        let root_res = instance
            .get_typed_func::<i32, i32>(&mut store, "state_root")?
            .call(&mut store, 384)?;
        assert_eq!(root_res, 0);
        let mut root = [0u8; 32];
        memory.read(&store, 384, &mut root)?;
        let direct_root = state.sparse_root_scoped(&ProgramId([1u8; 32]).0)?;
        assert_eq!(root, direct_root);
        Ok(())
    }

    #[test]
    fn crypto_host_apis() -> Result<()> {
        let (mut store, instance, memory, _, _) = build_store_with_instance()?;

        let input = b"crypto test";
        memory.write(&mut store, 0, input)?;
        // SHA-256
        let sha_len = instance
            .get_typed_func::<(i32, i32, i32, i32, i32), i32>(&mut store, "crypto_hash")?
            .call(&mut store, (1, 0, input.len() as i32, 128, 64))?;
        assert_eq!(sha_len, 32);
        let mut sha_out = [0u8; 32];
        memory.read(&store, 128, &mut sha_out)?;
        assert_eq!(sha_out, sha2::Sha256::digest(input).as_slice());

        // Keccak-256
        let keccak_len = instance
            .get_typed_func::<(i32, i32, i32, i32, i32), i32>(&mut store, "crypto_hash")?
            .call(&mut store, (2, 0, input.len() as i32, 256, 64))?;
        assert_eq!(keccak_len, 32);

        // secp256k1 sign/verify/recover
        let secp = Secp256k1::new();
        let secret = secp256k1::SecretKey::from_slice(&[2u8; 32]).unwrap();
        let pubkey = PublicKey::from_secret_key(&secp, &secret);
        let msg_hash = sha2::Sha256::digest(b"msg");
        memory.write(&mut store, 0, &msg_hash)?;
        let message = Message::from_digest_slice(&msg_hash).unwrap();
        let rec_sig = secp.sign_ecdsa_recoverable(&message, &secret);
        let (rec_id, sig_bytes64) = rec_sig.serialize_compact();
        let mut sig65 = [0u8; 65];
        sig65[..64].copy_from_slice(&sig_bytes64);
        sig65[64] = rec_id.to_i32() as u8;
        memory.write(&mut store, 64, &sig65)?;
        let uncompressed = pubkey.serialize_uncompressed();
        memory.write(&mut store, 136, &uncompressed[1..])?;

        let verify_res = instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut store, "crypto_verify")?
            .call(&mut store, (0, 64, 136))?;
        assert_eq!(verify_res, 0);

        let recover_res = instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut store, "crypto_recover")?
            .call(&mut store, (0, 64, 200))?;
        assert_eq!(recover_res, 0);
        let mut recovered = [0u8; 64];
        memory.read(&store, 200, &mut recovered)?;
        assert_eq!(recovered, uncompressed[1..]);

        Ok(())
    }
}
