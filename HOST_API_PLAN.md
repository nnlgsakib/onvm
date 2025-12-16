# ONVM Host API Expansion Plan

This document outlines a plan to expand the host API available to WebAssembly programs running on the ONVM runtime. The goal is to provide a richer set of capabilities to enable more complex and "arbitrary" code to run, while maintaining a high degree of security and control from the host.

## Design Philosophy

1.  **Capability-Based Security:** Programs are not given ambient authority. All access to external resources (filesystem, network, etc.) must be granted and scoped by the host.
2.  **Least Privilege:** By default, a program has no access to I/O. All permissions must be explicitly declared in the program's manifest.
3.  **Sandboxing:** All I/O is virtualized and sandboxed. For example, filesystem access is confined to a virtual root directory specific to the program.
4.  **Deterministic vs. Non-Deterministic APIs:** APIs are clearly separated into deterministic (e.g., pure computation, state access) and non-deterministic (e.g., network calls, timestamp). This is crucial for replayability and consensus. The manifest will declare which category of APIs the program intends to use.
5.  **Clear and Minimal Interface:** The API surface should be easy to understand and use, following the existing `snake_case` convention. Data is passed via pointers and lengths.
---
## Existing APIs for Reference

For completeness, the existing APIs are listed here.

### Blob Storage

- **`onvm_blob_read(id_ptr: i32, id_len: i32, out_ptr: i32, out_capacity: i32) -> i32`**
  - **Description:** Reads a blob by its ID.

### State Management

- **`onvm_state_put(key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32) -> i32`**
  - **Description:** Puts a key-value pair into the program's state.

- **`onvm_state_get(key_ptr: i32, key_len: i32, out_ptr: i32, out_cap: i32) -> i32`**
  - **Description:** Gets a value by its key from the program's state.

- **`onvm_state_root(out_ptr: i32) -> i32`**
  - **Description:** Gets the current state root of the program's isolated state.

---
---

## Proposed Host APIs

### 1. Filesystem API (Sandboxed)

A virtual filesystem will be provided to each program instance, rooted at a directory managed by the host. The manifest will specify whether the filesystem is needed and if it should be persistent or ephemeral.

- **`onvm_fs_open(path_ptr: i32, path_len: i32, flags: i32, mode: i32) -> i32`**
  - **Description:** Opens a file within the program's sandboxed directory.
  - **Parameters:**
    - `path_ptr`, `path_len`: Pointer and length for the file path string.
    - `flags`: Open flags (e.g., `O_RDONLY`, `O_WRONLY`, `O_CREAT`).
    - `mode`: File permissions (e.g., `0o755`), used if creating a new file.
  - **Returns:** A file descriptor (a non-negative `i32`) on success, or a negative error code.

- **`onvm_fs_read(fd: i32, out_ptr: i32, out_cap: i32) -> i32`**
  - **Description:** Reads from an open file descriptor into a buffer.
  - **Returns:** Number of bytes read on success, or a negative error code. `0` indicates EOF.

- **`onvm_fs_write(fd: i32, in_ptr: i32, in_len: i32) -> i32`**
  - **Description:** Writes data from a buffer to an open file descriptor.
  - **Returns:** Number of bytes written on success, or a negative error code.

- **`onvm_fs_seek(fd: i32, offset: i64, whence: i32) -> i64`**
  - **Description:** Moves the file offset.
  - **Parameters:**
    - `whence`: Specifies the origin for the seek (e.g., `SEEK_SET`, `SEEK_CUR`, `SEEK_END`).
  - **Returns:** The new offset from the beginning of the file on success, or a negative error code.

- **`onvm_fs_close(fd: i32) -> i32`**
  - **Description:** Closes a file descriptor.
  - **Returns:** `0` on success, or a negative error code.

- **`onvm_fs_stat(path_ptr: i32, path_len: i32, stat_buf_ptr: i32) -> i32`**
    - **Description:** Gets file status (size, modification time, etc.) and writes it to a struct in Wasm memory.
    - **Returns:** `0` on success, or a negative error code.

### 2. Networking API (Capability-Based)

Network access will be strictly controlled by the program's manifest, which must declare the domains or IP addresses it's allowed to connect to.

- **`onvm_net_socket_open(domain: i32, type: i32, protocol: i32) -> i32`**
  - **Description:** Creates a new socket. The host verifies the requested domain and type against the program's manifest.
  - **Parameters:**
    - `domain`: `AF_INET` or `AF_INET6`.
    - `type`: `SOCK_STREAM` or `SOCK_DGRAM`.
  - **Returns:** A socket descriptor on success, or a negative error code.

- **`onvm_net_connect(fd: i32, addr_ptr: i32, addr_len: i32) -> i32`**
  - **Description:** Connects a socket to a remote address. The host will intercept this call and verify that the target address is in the program's allowlist.
  - **Returns:** `0` on success, or a negative error code.

- **`onvm_net_send(fd: i32, in_ptr: i32, in_len: i32, flags: i32) -> i32`**
  - **Description:** Sends data on a connected socket.
  - **Returns:** Number of bytes sent on success, or a negative error code.

- **`onvm_net_recv(fd: i32, out_ptr: i32, out_cap: i32, flags: i32) -> i32`**
  - **Description:** Receives data from a socket.
  - **Returns:** Number of bytes received on success, or a negative error code.

- **`onvm_net_close(fd: i32) -> i32`**
  - **Description:** Closes a socket.
  - **Returns:** `0` on success, or a negative error code.

### 3. Cryptography API

Provides access to cryptographic primitives. This avoids the need for Wasm programs to bundle their own (potentially large and slow) crypto libraries.

- **`onvm_crypto_hash(algo: i32, in_ptr: i32, in_len: i32, out_ptr: i32, out_cap: i32) -> i32`**
  - **Description:** Hashes data using a specified algorithm.
  - **Parameters:**
    - `algo`: An integer representing the hash algorithm (e.g., `1` for SHA-256, `2` for Keccak-256).
  - **Returns:** The number of bytes in the hash digest on success, or a negative error code.

- **`onvm_crypto_secp256k1_verify(msg_hash_ptr: i32, sig_ptr: i32, pubkey_ptr: i32) -> i32`**
  - **Description:** Verifies a secp256k1 ECDSA signature.
  - **Parameters:**
    - `msg_hash_ptr`: Pointer to the 32-byte message hash.
    - `sig_ptr`: Pointer to the 65-byte signature (R, S, V).
    - `pubkey_ptr`: Pointer to the 64-byte uncompressed public key.
  - **Returns:** `0` if the signature is valid, `1` if invalid, and a negative error code on other failures.

- **`onvm_crypto_secp256k1_recover(msg_hash_ptr: i32, sig_ptr: i32, out_pubkey_ptr: i32) -> i32`**
  - **Description:** Recovers a public key from a secp256k1 signature and message hash.
  - **Returns:** `0` on success, writing the 64-byte public key to `out_pubkey_ptr`. Returns a negative error code on failure.

### 4. Environment & System API

Provides access to system information in a controlled manner.

- **`onvm_env_get_time() -> i64`**
  - **Description:** Returns the current system time as a Unix timestamp (nanoseconds).
  - **Returns:** A 64-bit integer representing nanoseconds since the Unix epoch. Access to this non-deterministic function must be declared in the manifest.

- **`onvm_env_get_random(out_ptr: i32, out_len: i32) -> i32`**
  - **Description:** Fills a buffer with cryptographically secure random bytes.
  - **Returns:** `0` on success, or a negative error code. This is also non-deterministic.

- **`onvm_env_log(msg_ptr: i32, msg_len: i32)`**
  - **Description:** Writes a message to the host's logging system, prefixed with the program ID. This is useful for debugging.

- **`onvm_env_get_arg(n: i32, out_ptr: i32, out_cap: i32) -> i32`**
    - **Description:** Gets the n-th command-line argument passed to the program.
    - **Returns:** The number of bytes written to the output buffer, `0` if the argument doesn't exist, or a negative error code.

- **`onvm_env_get_var(key_ptr: i32, key_len: i32, out_ptr: i32, out_cap: i32) -> i32`**
    - **Description:** Gets the value of an environment variable from a sandboxed set of variables provided by the host.
    - **Returns:** The number of bytes written to the output buffer, `0` if the variable is not found, or a negative error code.



### 5. GPU Compute API (Experimental)

This API provides access to GPU for general-purpose computing (GPGPU). Access must be explicitly granted in the manifest. The API is modeled after CUDA's driver API but is designed to be abstract enough for other backends in the future.

**Note:** Pointers in this API (`dev_ptr`) are opaque handles managed by the host and refer to GPU device memory, not Wasm memory.

- **`onvm_gpu_device_count() -> i32`**
  - **Description:** Returns the number of available GPU devices on the host.
  - **Returns:** The number of detected devices, or a negative error code.

- **`onvm_gpu_malloc(device_id: i32, size: i64, dev_ptr_out: i32) -> i32`**
  - **Description:** Allocates memory on the specified GPU device.
  - **Parameters:**
    - `device_id`: The ID of the target device.
    - `size`: The number of bytes to allocate.
    - `dev_ptr_out`: A pointer in Wasm memory where the resulting 64-bit device pointer handle will be written.
  - **Returns:** `0` on success, or a negative error code.

- **`onvm_gpu_free(device_id: i32, dev_ptr: i64) -> i32`**
  - **Description:** Frees memory on the GPU device.
  - **Parameters:**
    - `device_id`: The ID of the target device.
    - `dev_ptr`: The device pointer handle to free.
  - **Returns:** `0` on success, or a negative error code.

- **`onvm_gpu_memcpy_host_to_device(device_id: i32, dest_dev_ptr: i64, src_wasm_ptr: i32, count: i64) -> i32`**
  - **Description:** Copies data from Wasm memory (host) to GPU memory (device).
  - **Parameters:**
    - `dest_dev_ptr`: The destination pointer handle in the device's memory.
    - `src_wasm_ptr`: The source pointer in the Wasm instance's memory.
    - `count`: The number of bytes to copy.
  - **Returns:** `0` on success, or a negative error code.

- **`onvm_gpu_memcpy_device_to_host(device_id: i32, dest_wasm_ptr: i32, src_dev_ptr: i64, count: i64) -> i32`**
  - **Description:** Copies data from GPU memory (device) to Wasm memory (host).
  - **Parameters:**
    - `dest_wasm_ptr`: The destination pointer in the Wasm instance's memory.
    - `src_dev_ptr`: The source pointer handle in the device's memory.
    - `count`: The number of bytes to copy.
  - **Returns:** `0` on success, or a negative error code.

- **`onvm_gpu_load_kernel(device_id: i32, kernel_blob_id_ptr: i32, kernel_blob_id_len: i32) -> i32`**
  - **Description:** Loads a pre-compiled compute kernel onto a GPU device. The kernel itself is stored in the blob store. For CUDA, this would be a PTX file.
  - **Parameters:**
    - `kernel_blob_id_ptr`, `kernel_blob_id_len`: Pointer and length for the blob ID of the kernel.
  - **Returns:** A kernel handle (an `i32`) on success, or a negative error code.

- **`onvm_gpu_launch_kernel(device_id: i32, kernel_handle: i32, func_name_ptr: i32, func_name_len: i32, grid_x: u32, grid_y: u32, grid_z: u32, block_x: u32, block_y: u32, block_z: u32, args_ptr: i32, args_len: i32) -> i32`**
  - **Description:** Launches a loaded kernel on the GPU.
  - **Parameters:**
    - `kernel_handle`: The handle returned by `onvm_gpu_load_kernel`.
    - `func_name_ptr`, `func_name_len`: The name of the kernel function to launch.
    - `grid_x, y, z`: The dimensions of the grid of thread blocks.
    - `block_x, y, z`: The dimensions of the thread block.
    - `args_ptr`, `args_len`: A pointer to an array of device pointer handles (`i64`) to be passed as arguments to the kernel.
  - **Returns:** `0` on success, or a negative error code.

- **`onvm_gpu_synchronize(device_id: i32) -> i32`**
  - **Description:** Blocks the calling thread until all preceding commands in the GPU's command stream have completed.
  - **Returns:** `0` on success, or a negative error code.
