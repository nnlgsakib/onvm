use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

static OUT_BUF: OnceCell<Mutex<Vec<u8>>> = OnceCell::new();

#[derive(Debug, Deserialize)]
struct JobInput {
    task: String,
    iterations: Option<u32>,
    data: Option<String>,
}

#[derive(Debug, Serialize)]
struct JobOutput {
    status: String,
    result: String,
    iterations_completed: u32,
    fuel_estimate: String,
}

#[no_mangle]
pub extern "C" fn onvm_main(ptr: i32, len: i32) -> (i32, i32) {
    let input = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
    
    let response = match process_job(input) {
        Ok(output) => output,
        Err(e) => serde_json::to_vec(&serde_json::json!({
            "status": "error",
            "result": e,
            "iterations_completed": 0,
            "fuel_estimate": "unknown"
        }))
        .unwrap_or_else(|_| b"{\"status\":\"error\",\"result\":\"serialization failed\"}".to_vec()),
    };

    let buf = OUT_BUF.get_or_init(|| Mutex::new(Vec::with_capacity(16 * 1024)));
    let mut guard = buf.lock().unwrap();
    guard.clear();
    guard.extend_from_slice(&response);
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

fn process_job(input: &[u8]) -> Result<Vec<u8>, String> {
    let text = std::str::from_utf8(input).map_err(|e| format!("utf8 error: {}", e))?;
    let job_input: JobInput =
        serde_json::from_str(text).map_err(|e| format!("invalid json: {}", e))?;

    let iterations = job_input.iterations.unwrap_or(10);

    let result = match job_input.task.as_str() {
        "compute" => compute_task(iterations, &job_input.data),
        "hash" => hash_task(iterations, &job_input.data),
        "transform" => transform_task(&job_input.data),
        "stress" => stress_task(iterations),
        _ => Err(format!("unknown task: {}", job_input.task)),
    }?;

    let output = JobOutput {
        status: "success".to_string(),
        result,
        iterations_completed: iterations,
        fuel_estimate: estimate_fuel(iterations),
    };

    serde_json::to_vec(&output).map_err(|e| format!("serialize error: {}", e))
}

fn compute_task(iterations: u32, data: &Option<String>) -> Result<String, String> {
    let base = data
        .as_ref()
        .and_then(|d| d.parse::<u64>().ok())
        .unwrap_or(1);
    
    let mut result = base;
    for i in 1..=iterations {
        result = result.wrapping_mul(i as u64).wrapping_add(i as u64);
    }
    
    Ok(format!("computed result: {}", result))
}

fn hash_task(iterations: u32, data: &Option<String>) -> Result<String, String> {
    let input = data.as_deref().unwrap_or("default-data");
    let mut hash_result = 0u64;
    
    for i in 0..iterations {
        for (idx, byte) in input.as_bytes().iter().enumerate() {
            hash_result = hash_result
                .wrapping_mul(31)
                .wrapping_add(*byte as u64)
                .wrapping_add(i as u64)
                .wrapping_add(idx as u64);
        }
    }
    
    Ok(format!("hash: {:016x}", hash_result))
}

fn transform_task(data: &Option<String>) -> Result<String, String> {
    let input = data.as_deref().unwrap_or("hello world");
    
    let transformed: String = input
        .chars()
        .enumerate()
        .map(|(i, c)| {
            if i % 2 == 0 {
                c.to_ascii_uppercase()
            } else {
                c.to_ascii_lowercase()
            }
        })
        .collect();
    
    Ok(format!("transformed: {}", transformed))
}

fn stress_task(iterations: u32) -> Result<String, String> {
    let mut accumulator: Vec<u64> = Vec::new();
    
    for i in 0..iterations.min(1000) {
        accumulator.push(fibonacci(i % 20));
    }
    
    let sum: u64 = accumulator.iter().sum();
    
    Ok(format!(
        "stress test completed: {} iterations, sum: {}",
        iterations, sum
    ))
}

fn fibonacci(n: u32) -> u64 {
    if n <= 1 {
        return n as u64;
    }
    let mut a = 0u64;
    let mut b = 1u64;
    for _ in 2..=n {
        let tmp = a.wrapping_add(b);
        a = b;
        b = tmp;
    }
    b
}

fn estimate_fuel(iterations: u32) -> String {
    let base_fuel = 10_000;
    let per_iteration = 1_000;
    let estimated = base_fuel + (iterations as u64 * per_iteration);
    format!("~{} fuel units", estimated)
}
