use base64::Engine;
use blake3::Hasher;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// Shared output buffer reused for responses.
static OUT_BUF: OnceCell<Mutex<Vec<u8>>> = OnceCell::new();

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
enum Request {
    Put { key: String, data_base64: String },
    Get { key: String },
    List,
    Clear,
    Stats,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
enum Response {
    OkPut { key: String, size: usize, checksum: String },
    OkGet { key: String, size: usize, checksum: String, data_base64: String },
    OkList { entries: Vec<Entry> },
    OkClear { removed: usize },
    OkStats { total_keys: usize, total_bytes: usize, checksum: String },
    Err { message: String },
}

#[derive(Debug, Serialize)]
struct Entry {
    key: String,
    size: usize,
    checksum: String,
}

#[no_mangle]
pub extern "C" fn onvm_main(ptr: i32, len: i32) -> (i32, i32) {
    // Safety: host guarantees the range is valid.
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let response = match handle_request(input) {
        Ok(resp) => resp,
        Err(msg) => Response::Err { message: msg },
    };

    let encoded = serde_json::to_vec(&response).unwrap_or_else(|_| b"{\"status\":\"error\",\"message\":\"serialize\"}".to_vec());
    let buf = OUT_BUF.get_or_init(|| Mutex::new(Vec::with_capacity(16 * 1024)));
    let mut guard = buf.lock().unwrap();
    guard.clear();
    guard.extend_from_slice(&encoded);
    let out_ptr = guard.as_ptr() as i32;
    let out_len = guard.len() as i32;
    std::mem::forget(guard);
    (out_ptr, out_len)
}

#[no_mangle]
pub extern "C" fn onvm_last_result() -> (i32, i32) {
    if let Some(buf) = OUT_BUF.get() {
        let guard = buf.lock().unwrap();
        let ptr = guard.as_ptr() as i32;
        let len = guard.len() as i32;
        std::mem::forget(guard);
        (ptr, len)
    } else {
        (0, 0)
    }
}

fn handle_request(input: &[u8]) -> Result<Response, String> {
    let req: Request = serde_json::from_slice(input).map_err(|e| format!("invalid json: {e}"))?;
    let b64 = &base64::engine::general_purpose::STANDARD;

    match req {
        Request::Put { key, data_base64 } => {
            let data = b64
                .decode(data_base64.as_bytes())
                .map_err(|e| format!("base64 decode: {e}"))?;
            state_put(key.as_bytes(), &data).map_err(|e| format!("state put: {e}"))?;
            let checksum = blake3_hex(&data);
            let size = data.len();
            update_key_index(b64, &key)?;
            Ok(Response::OkPut { key, size, checksum })
        }
        Request::Get { key } => {
            let data = state_get_with_resize(key.as_bytes())?;
            let data_len = data.len();
            let checksum = blake3_hex(&data);
            let encoded = b64.encode(data);
            Ok(Response::OkGet {
                key,
                size: data_len,
                checksum,
                data_base64: encoded,
            })
        }
        Request::List => {
            let keys = load_key_index(b64)?;
            let mut entries: Vec<_> = keys
                .into_iter()
                .filter_map(|k| {
                    state_get_with_resize(k.as_bytes())
                        .ok()
                        .map(|v| Entry {
                            key: k.clone(),
                            size: v.len(),
                            checksum: blake3_hex(&v),
                        })
                })
                .collect();
            entries.sort_by(|a, b| a.key.cmp(&b.key));
            Ok(Response::OkList { entries })
        }
        Request::Clear => {
            let keys = load_key_index(b64)?;
            for k in &keys {
                let _ = state_put(k.as_bytes(), &[]);
            }
            state_put(KEY_INDEX_NAME.as_bytes(), &b64.encode(Vec::<u8>::new()).into_bytes())?;
            Ok(Response::OkClear {
                removed: keys.len(),
            })
        }
        Request::Stats => {
            let keys = load_key_index(b64)?;
            let mut total_bytes = 0usize;
            let mut combined = Vec::new();
            for k in keys.iter() {
                if let Ok(v) = state_get_with_resize(k.as_bytes()) {
                    total_bytes += v.len();
                    combined.extend_from_slice(k.as_bytes());
                    combined.extend_from_slice(&v);
                }
            }
            let checksum = blake3_hex(&combined);
            Ok(Response::OkStats {
                total_keys: keys.len(),
                total_bytes,
                checksum,
            })
        }
    }
}

const KEY_INDEX_NAME: &str = "__keys";

fn state_put(key: &[u8], value: &[u8]) -> Result<(), String> {
    let rc = unsafe {
        onvm_state_put(
            buf_ptr(key),
            key.len() as i32,
            buf_ptr(value),
            value.len() as i32,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!("state put failed: {rc}"))
    }
}

fn state_get_with_resize(key: &[u8]) -> Result<Vec<u8>, String> {
    let mut cap = 1024;
    for _ in 0..5 {
        let mut buf = vec![0u8; cap];
        let len = unsafe {
            onvm_state_get(
                buf_ptr(key),
                key.len() as i32,
                buf.as_mut_ptr() as i32,
                cap as i32,
            )
        };
        if len == 0 {
            return Err("missing key".into());
        }
        if len > 0 {
            buf.truncate(len as usize);
            return Ok(buf);
        }
        // negative means needed size
        cap = (-len) as usize;
    }
    Err("unable to fetch value".into())
}

fn load_key_index(b64: &base64::engine::general_purpose::GeneralPurpose) -> Result<Vec<String>, String> {
    match state_get_with_resize(KEY_INDEX_NAME.as_bytes()) {
        Ok(bytes) => {
            if bytes.is_empty() {
                return Ok(Vec::new());
            }
            let decoded = b64
                .decode(bytes)
                .map_err(|e| format!("decode keys: {e}"))?;
            let as_str = core::str::from_utf8(&decoded).map_err(|e| format!("utf8 keys: {e}"))?;
            Ok(as_str
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect())
        }
        Err(_) => Ok(Vec::new()),
    }
}

fn update_key_index(
    b64: &base64::engine::general_purpose::GeneralPurpose,
    key: &str,
) -> Result<(), String> {
    let mut keys = load_key_index(b64)?;
    if !keys.contains(&key.to_string()) {
        keys.push(key.to_string());
        keys.sort();
        let joined = keys.join(",");
        let enc = b64.encode(joined.as_bytes());
        state_put(KEY_INDEX_NAME.as_bytes(), enc.as_bytes())
    } else {
        Ok(())
    }
}

fn blake3_hex(data: &[u8]) -> String {
    let mut h = Hasher::new();
    h.update(data);
    h.finalize().to_hex().to_string()
}

fn buf_ptr(slice: &[u8]) -> i32 {
    slice.as_ptr() as i32
}

extern "C" {
    fn onvm_state_put(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32) -> i32;
    fn onvm_state_get(key_ptr: i32, key_len: i32, out_ptr: i32, out_cap: i32) -> i32;
    fn onvm_state_root(out_ptr: i32) -> i32;
}
