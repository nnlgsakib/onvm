# Repository Guidelines

This document is a concise guide for contributing to the ONVM Rust codebase. It highlights how the project is organized, how to build and test it, and what is expected for code and reviews.

## Project Structure & Module Organization
- `src/main.rs` and `src/cli.rs` define the CLI entrypoints (`init`, `run-node`, `upload-blob`, `deploy`, `execute`, `get-blob`, `program-info`).
- Core runtime: `src/node.rs` plus modules for networking (`src/network`), consensus (`src/consensus`), execution (`src/execution`), storage (`src/storage`), RPC (`src/rpc`), cryptography (`src/crypto`), and block assembly (`src/blocksig`).
- Shared types live in `src/types.rs`; configuration defaults in `src/config`.
- Sample WASM program is under `wasm_programs/echo`; `target/` holds build artifacts; `input.bin` is a sample payload.

## Build, Test, and Development Commands
- `cargo fmt` — format the codebase.
- `cargo clippy --all-targets --all-features` — lint with warnings treated seriously; fix or `#[allow]` with rationale.
- `cargo test --all-targets` — run unit/integration tests; add focused filters during development (e.g., `cargo test runtime`).
- `cargo build` (or `--release`) — compile the node.
- `cargo run -- --help` to inspect CLI; typical flows: `cargo run -- init --data-dir ./data` then `cargo run -- run-node --data-dir ./data --listen /ip4/0.0.0.0/tcp/37000 --rpc 127.0.0.1:8080`.

## Coding Style & Naming Conventions
- Rust 2021 with 4-space indentation and `snake_case` for functions/modules; structs and enums use `PascalCase`.
- Prefer explicit error contexts via `anyhow::Context` and avoid unchecked `unwrap`/`expect` in non-test code.
- Keep modules cohesive (network/consensus/execution boundaries) and place helpers near their domain.
- Run `cargo fmt` and `cargo clippy` before pushing; align logging with `tracing` levels already in use.

## Testing Guidelines
- Co-locate unit tests in `#[cfg(test)]` modules next to the code; integration tests may live in `tests/` if broader coverage is needed.
- Name tests after behavior (e.g., `handles_empty_batch`, `rejects_invalid_multiaddr`).
- When adding features, cover happy path plus obvious failure cases; document any intentional gaps in the PR.

## Commit & Pull Request Guidelines
- Git history shows short, action-led summaries (`added ...`, `fixed ...`); prefer concise imperative lines (≤72 chars), optionally referencing issues (`Fix: ... (#123)`).
- PRs should include: a clear description, steps/commands used for testing (paste `cargo test`/`cargo clippy` output when relevant), and any protocol/RPC changes.
- Add screenshots or sample CLI invocations when behavior changes, and call out migration or config impacts on `config.toml`/data directories.

## Security & Configuration Tips
- `onvm init` generates `./data/identity`; do not commit keys or local `data` directories.
- RPC endpoints default to `127.0.0.1:8080`; wrap them with `http://` or use `:8080` shorthand. Normalize multiaddrs before sharing.
- Keep WASM inputs and blobs deterministic; prefer pinned versions and avoid non-reproducible randomness in committed assets.
