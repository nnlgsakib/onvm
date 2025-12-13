#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::mem;

use spin::{Mutex, Once};

/// Global output buffer – created once, reused forever
static OUT_BUF: Once<Mutex<Vec<u8>>> = Once::new();

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

/// ONVM entrypoint
/// Input : ptr:i32, len:i32 → raw bytes in linear memory
/// Output: (ptr, len)
#[no_mangle]
pub extern "C" fn onvm_main(ptr: i32, len: i32) -> (i32, i32) {
    // Safety: host guarantees the memory range is valid
    let input = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };

    let buffer = OUT_BUF.call_once(|| Mutex::new(Vec::with_capacity(4096)));

    let mut out = buffer.lock();
    out.clear();

    // This is the only line that needed fixing for Rust 1.82+
    out.extend(input.iter().map(|&b| b.to_ascii_uppercase()));

    let result_ptr = out.as_ptr() as i32;
    let result_len = out.len() as i32;

    mem::forget(out);
    (result_ptr, result_len)
}

#[no_mangle]
pub extern "C" fn onvm_last_result() -> (i32, i32) {
    if let Some(buffer) = OUT_BUF.get() {
        let out = buffer.lock();
        let ptr = out.as_ptr() as i32;
        let len = out.len() as i32;
        mem::forget(out);
        (ptr, len)
    } else {
        (0, 0)
    }
}
