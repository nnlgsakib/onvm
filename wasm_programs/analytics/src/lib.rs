use base64::Engine;
use blake3::Hasher;
use once_cell::sync::OnceCell;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

static OUT_BUF: OnceCell<Mutex<Vec<u8>>> = OnceCell::new();

#[derive(Debug, Deserialize)]
struct Input {
    text: Option<String>,
    numbers: Option<Vec<f64>>,
    #[serde(default)]
    include_compressed: bool,
}

#[derive(Debug, Serialize)]
struct AnalysisOutput {
    blake3_hex: String,
    text_len: usize,
    token_count: usize,
    top_tokens: Vec<(String, usize)>,
    numbers: Option<NumberStats>,
    compressed_len: Option<usize>,
    compressed_b64: Option<String>,
}

#[derive(Debug, Serialize)]
struct NumberStats {
    count: usize,
    mean: f64,
    variance: f64,
    min: f64,
    max: f64,
}

#[no_mangle]
pub extern "C" fn onvm_main(ptr: i32, len: i32) -> (i32, i32) {
    // Safety: caller guarantees the memory range is valid.
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let response = match process(input) {
        Ok(buf) => buf,
        Err(e) => serde_json::to_vec(&serde_json::json!({ "error": e }))
            .unwrap_or_else(|_| b"{\"error\":\"serialization\"}".to_vec()),
    };

    let storage = OUT_BUF.get_or_init(|| Mutex::new(Vec::with_capacity(8192)));
    let mut out = storage.lock().unwrap();
    out.clear();
    out.extend_from_slice(&response);
    let ptr = out.as_ptr() as i32;
    let len = out.len() as i32;
    std::mem::forget(out);
    (ptr, len)
}

#[no_mangle]
pub extern "C" fn onvm_last_result() -> (i32, i32) {
    if let Some(cell) = OUT_BUF.get() {
        let out = cell.lock().unwrap();
        let ptr = out.as_ptr() as i32;
        let len = out.len() as i32;
        std::mem::forget(out);
        (ptr, len)
    } else {
        (0, 0)
    }
}

fn process(input: &[u8]) -> Result<Vec<u8>, String> {
    let text = String::from_utf8(input.to_vec()).map_err(|e| format!("utf8: {e}"))?;
    let parsed: Input =
        serde_json::from_str(&text).map_err(|e| format!("invalid json input: {e}"))?;

    let raw_text = parsed.text.unwrap_or_default();
    let tokens = tokenize(&raw_text);
    let top_tokens = top_n(&tokens, 5);

    let hash_hex = {
        let mut h = Hasher::new();
        h.update(raw_text.as_bytes());
        h.finalize().to_hex().to_string()
    };

    let (compressed_len, compressed_b64) = if parsed.include_compressed {
        let compressed = lz4_flex::block::compress_prepend_size(raw_text.as_bytes());
        let b64 = base64::engine::general_purpose::STANDARD.encode(&compressed);
        (Some(compressed.len()), Some(b64))
    } else {
        (None, None)
    };

    let numbers = parsed.numbers.as_ref().map(compute_number_stats);

    let output = AnalysisOutput {
        blake3_hex: hash_hex,
        text_len: raw_text.len(),
        token_count: tokens.values().sum(),
        top_tokens,
        numbers,
        compressed_len,
        compressed_b64,
    };

    serde_json::to_vec(&output).map_err(|e| format!("serialize: {e}"))
}

fn tokenize(text: &str) -> HashMap<String, usize> {
    let re = Regex::new(r"[A-Za-z0-9]+").unwrap();
    let mut counts = HashMap::new();
    for cap in re.find_iter(text) {
        let entry = counts.entry(cap.as_str().to_ascii_lowercase()).or_insert(0);
        *entry += 1;
    }
    counts
}

fn top_n(map: &HashMap<String, usize>, n: usize) -> Vec<(String, usize)> {
    let mut items: Vec<_> = map.iter().map(|(k, &v)| (k.clone(), v)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.truncate(n);
    items
}

fn compute_number_stats(nums: &Vec<f64>) -> NumberStats {
    let count = nums.len();
    if count == 0 {
        return NumberStats {
            count: 0,
            mean: 0.0,
            variance: 0.0,
            min: 0.0,
            max: 0.0,
        };
    }
    let sum: f64 = nums.iter().sum();
    let mean = sum / count as f64;
    let variance = if count > 1 {
        nums.iter()
            .map(|v| {
                let d = v - mean;
                d * d
            })
            .sum::<f64>()
            / (count as f64 - 1.0)
    } else {
        0.0
    };
    let min = nums
        .iter()
        .cloned()
        .fold(f64::INFINITY, |a, b| a.min(b));
    let max = nums
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, |a, b| a.max(b));
    NumberStats {
        count,
        mean,
        variance,
        min,
        max,
    }
}
