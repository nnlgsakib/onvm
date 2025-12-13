# ONVM – Open Network Virtual Machine

**ONVM** is a peer-to-peer decentralized compute platform that combines a content-addressed storage layer, a verifiable execution runtime powered by WebAssembly, and a gossip-based consensus mechanism to create a minimal, trustless virtual machine for distributed applications.

---

## Overview

ONVM nodes form a self-organizing network where participants:
- **Store** content-addressed blobs (data, programs) with chunked, merkle-verified integrity
- **Deploy** WebAssembly programs with versioned, deterministic execution
- **Execute** computations against isolated per-program state stores
- **Replicate** operations through a directed acyclic graph (DAG) consensus protocol
- **Synchronize** state, programs, and blobs across peers using gossip and Kademlia DHT

Each operation (blob publish, program deploy, execution) is embedded in a DAG node, gossiped to peers, and eventually converges network-wide, ensuring eventual consistency without centralized coordination.

---

## Architecture

### Core Components

| Module | Purpose |
|--------|---------|
| **`src/node.rs`** | Top-level orchestrator that wires together networking, consensus, execution, and storage |
| **`src/network/`** | libp2p-based P2P stack (gossipsub, Kademlia, mDNS, request-response) |
| **`src/consensus/`** | DAG engine for operation ordering, replication, and peer synchronization |
| **`src/execution/`** | Wasmtime-based runtime with host functions for blob/state access |
| **`src/storage/`** | BlobStore (chunked + merkle roots) and StateStore (program-scoped KV) |
| **`src/rpc/`** | Axum-based HTTP API for blob upload, program deployment, and execution |
| **`src/crypto/`** | Ed25519 identity keys and Blake3 hashing |

### Data Flow

```
CLI → RPC → Node → Consensus (DAG) → Network (gossip)
                  ↓
            ExecutionEngine → StateStore
                  ↓
            BlobStore (chunked, merkle-verified)
```

1. **Upload Blob**: client POSTs blob → RPC → BlobStore → DAG node → gossip to peers
2. **Deploy Program**: client POSTs wasm → ProgramStore → DAG node → gossip
3. **Execute**: client POSTs program_id + input → ExecutionEngine (fuel-metered wasm) → StateStore → DAG node → gossip

---

## Building & Running

### Prerequisites
- Rust **1.82+** (edition 2021)
- `cargo`, `cargo-fmt`, `cargo-clippy`

### Build
```bash
cargo build --release
```

### Initialize a Node
```bash
cargo run -- init --data-dir ./data
```
Generates Ed25519 identity keys in `./data/identity` and a default `config.toml`.

### Run a Node
```bash
cargo run -- run-node \
  --data-dir ./data \
  --listen /ip4/0.0.0.0/tcp/37000 \
  --rpc 127.0.0.1:8080 \
  --min-peers 1 \
  --blob-sync-mode full
```
- `--listen`: P2P multiaddr (will auto-increment port on conflict)
- `--rpc`: HTTP API bind address (`:8080` shorthand supported)
- `--min-peers`: minimum connected peers before accepting executions
- `--blob-sync-mode`: `full` (replicate data) or `metadata` (index only)

### Upload a Blob
```bash
cargo run -- upload-blob \
  --rpc :8080 \
  --file input.bin \
  --mime application/octet-stream
```
Returns a 64-char hex **BlobId**.

### Deploy a WASM Program
```bash
# Build a sample program first
cd wasm_programs/echo
cargo build --target wasm32-unknown-unknown --release
cd ../..

cargo run -- deploy \
  --rpc :8080 \
  --file wasm_programs/echo/target/wasm32-unknown-unknown/release/echo.wasm \
  --entrypoint onvm_main \
  --blob-refs <blob-id-1> <blob-id-2> \
  --salt <base64-salt>
```
Returns a hex **ProgramId** (deterministic hash of wasm + salt).

### Execute a Program
```bash
cargo run -- execute \
  --rpc :8080 \
  --program-id <program-id> \
  --input input.bin
```
Prints base64-decoded output to stdout; use `--json` for structured response.

### Fetch a Blob
```bash
cargo run -- get-blob \
  --rpc :8080 \
  --id <blob-id> \
  --out downloaded.bin
```

### Inspect a Program
```bash
cargo run -- program-info \
  --rpc :8080 \
  --id <program-id>
```
Returns JSON metadata (publisher, entrypoint, blob_refs, size).

---

## WASM Programs

ONVM supports WebAssembly programs compiled to `wasm32-unknown-unknown` (or `wasm32-wasi` for broader stdlib).

### Entrypoint Signatures
Programs export one of:
- `fn onvm_main(ptr: i32, len: i32) -> (i32, i32)` (multi-value)
- `fn onvm_main(ptr: i32, len: i32) -> i64` (packed pointer+length)
- `fn onvm_main(sret_ptr: i32, input_ptr: i32, input_len: i32)` (struct return)

### Host Functions
| Function | Signature | Purpose |
|----------|-----------|---------|
| **`onvm_blob_read`** | `(id_ptr, id_len, out_ptr, out_cap) -> i32` | Read a blob by hex ID |
| **`onvm_state_put`** | `(key_ptr, key_len, val_ptr, val_len) -> i32` | Write to program-scoped state |
| **`onvm_state_get`** | `(key_ptr, key_len, out_ptr, out_cap) -> i32` | Read from state (returns len or -needed) |
| **`onvm_state_root`** | `(out_ptr) -> i32` | Get 32-byte merkle root of current state |

### Sample Programs
Located in `wasm_programs/`:

1. **`echo`** – Uppercases input bytes (no-std, minimal allocator demo)
2. **`kvstore`** – JSON-based key-value store with `put`/`get`/`list`/`clear`/`stats` operations
3. **`analytics`** – Text analysis: Blake3 hash, token counting, LZ4 compression, number stats

Build with:
```bash
cd wasm_programs/<program-name>
cargo build --target wasm32-unknown-unknown --release
```

Resulting `.wasm` files are in `target/wasm32-unknown-unknown/release/`.

---

## Testing

```bash
cargo test --all-targets
cargo clippy --all-targets --all-features
cargo fmt --check
```

See `AGENTS.md` for full build/test conventions.

---

## Configuration

Default settings in `src/config/mod.rs`:
- **Block params**: 512 max ops/block, 500ms slot duration, 1 min op/block
- **Genesis**: empty state root
- **Network**: 1 min peer, bootnodes list (placeholder)

Override by editing `./data/config.toml` after `init`.

---

## Networking

- **Gossipsub**: Topic-based pub/sub for blobs, programs, and DAG operations
- **Kademlia DHT**: Provider records for content discovery
- **mDNS**: Local peer discovery (auto-dials LAN nodes)
- **Request-Response**: Direct blob/program/execution transfer between peers
- **Multiaddr**: `/ip4/0.0.0.0/tcp/37000` or custom (auto-increments on bind conflict)

---

## Security

- **Identity**: Ed25519 keypair stored in `./data/identity` (never commit)
- **Hashing**: Blake3 for content IDs, merkle roots, and DAG node IDs
- **Sandboxing**: Wasmtime fuel metering (default 50M instructions/exec), no filesystem access by default
- **Determinism**: Programs must be reproducible; avoid randomness in committed blobs

---

## Roadmap

- [ ] Byzantine fault tolerance (signature verification, quorum thresholds)
- [ ] Advanced state sync (snapshot transfer, incremental merkle proofs)
- [ ] Persistent execution scheduler (async task queue across node restarts)
- [ ] Program versioning and upgrade paths
- [ ] Gas/fuel economics for resource metering
- [ ] Multi-program orchestration (program calls program)

---

## License

Licensed under the Apache License, Version 2.0. See `LICENSE` for details.

---

## Contributing

Contributions are welcome! Please follow these guidelines:

### Code Style
- Use `snake_case` for functions/modules, `PascalCase` for structs/enums
- 4-space indentation (run `cargo fmt` before committing)
- Provide explicit error contexts using `anyhow::Context`
- Avoid unchecked `unwrap`/`expect` in production code

### Commit Conventions
- Use short, imperative commit messages (≤72 chars)
- Reference issues when applicable (e.g., `Fix: handle empty blobs (#42)`)
- Focus on "why" rather than "what"

### Pull Requests
- Run `cargo fmt && cargo clippy --all-targets --all-features` before submitting
- Ensure `cargo test --all-targets` passes
- Include test coverage for new features
- Document breaking changes to RPC endpoints or config files
- Add sample CLI invocations or screenshots for user-facing changes

---

## Contact

For questions, issues, or contributions, see the repository's issue tracker.
