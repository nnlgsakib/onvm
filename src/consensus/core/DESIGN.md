# ONVM Consensus Core (Design Blueprint)

This document describes the target consensus design for ONVM so that **each program has a single, finalized state** across the network, and nodes can safely sync state without trusting arbitrary peers.

The current codebase contains a “DAG engine” that orchestrates networking, object distribution, execution, and receipt gossip. The goal of the `core/` refactor is to evolve that into a real consensus core with:

- deterministic ordering of state transitions per program
- committee-based finality (BFT)
- authenticated state replication and snapshots
- durable storage of consensus artifacts (blocks, QCs, committee epochs)

## Goals

- **Single state per program**: all honest nodes converge on the same finalized `state_root` sequence for a given `ProgramId`.
- **Finality**: once a transition is committed, it will not be reverted (within the fault model).
- **Safety-first sync**: nodes never apply remote state writes unless they are justified by finality proofs.
- **Permissioned-by-default**: initial design assumes a configured/known validator set; sybil resistance is out of scope for v1.

## Threat Model (v1)

- Up to `f` Byzantine validators in a committee of `n` with threshold `2f+1`.
- Network is asynchronous; messages may be delayed/reordered/dropped.
- Attackers may spam gossip, send invalid state deltas, or attempt to equivocate.

## Core Data Model

### Receipt (already exists)

`ExecutionReceipt` is the canonical statement of a state transition:

- `state_root_in` / `state_root_out`
- `write_digest` (hash of the state delta)
- `inputs_hash`, `wasm_code_hash`, `wasm_env_hash`

### Block (new)

A **program block** is a linear append-only chain per program (not a global chain):

- `program_id`
- `height`
- `parent` (hash of parent block)
- `receipt` (or receipt id)
- `state_delta` (the actual writes; optional for metadata-only replication)
- `timestamp_ms`, `proposer`

### QC / Finality Proof (new)

A **Quorum Certificate (QC)** is an aggregated BLS signature over a block hash:

- `block_hash`
- `epoch`
- `signer_bitmap`
- `aggregate_signature`

Finality is derived from a HotStuff-style commit rule (see below).

## Committee & Epochs

### Identity binding

Each validator has:

- **Node identity**: Ed25519 (`NodeId` already derives from the public key)
- **Consensus key**: BLS public key used for votes/QCs

Nodes must prove binding between the two keys (ed25519 signature over the BLS public key and domain separation tag).

### Initial committee (per program)

The initial committee is anchored in the program’s genesis/manifest:

- Program deploy includes a validator list (NodeId + BLS pubkey + weight) and a threshold.
- The manifest is signed by the deployer’s Ed25519 key.

Peers accept epoch-0 committee only if it matches the manifest signature.

### Committee changes

Committee updates happen via a special **CommitteeUpdate block** that is finalized by the current committee; the new committee becomes active at `epoch+1`.

## Consensus Protocol (HotStuff-like, per program)

For each program:

1. **Leader proposes** block `B(h)` extending the latest certified block.
2. **Validators verify**:
   - program + input objects are available (or fetchable)
   - `receipt.state_root_in` matches their locally finalized root at `h-1`
   - `write_digest` matches the provided `state_delta` (if present)
   - applying `state_delta` yields `state_root_out`
3. Validators send a **vote** (BLS signature) for `hash(B(h))` to the leader.
4. Leader aggregates votes into a **QC** once threshold is reached and broadcasts `QC(h)`.
5. **Finality rule**: when there is a 3-chain of QCs (classic HotStuff), the middle block becomes committed/finalized.

This yields deterministic ordering and finality under `2f+1` honesty.

## State Replication (Secure)

### Delta replication

State is applied only when:

- the block is finalized (has finality proof), and
- `state_delta` hashes to `receipt.write_digest`, and
- applying it yields `receipt.state_root_out`.

### Snapshots

For fast sync:

- validators periodically produce **snapshots** at height `H`:
  - snapshot root = finalized state root at `H`
  - chunked snapshot data in `UnifiedStore`
- snapshot is accepted only if the root is finalized by consensus at `H`.

## Data Availability

Blocks reference objects (program code, input blobs). Voting requires availability:

- use `UnifiedStore` + `ChunkDistributor` to fetch missing objects/chunks
- future: add erasure coding + provider liveness scoring

## Storage Requirements

Persist:

- per-program finalized head (height, root, block hash)
- blocks + QCs
- committee epochs (validator set + threshold + aggregate key)
- replay-protection for votes and proposals

## Implementation Plan (incremental)

1. **Refactor**: move consensus-related files to `src/consensus/core` (done).
2. **Identity**: make BLS consensus key stable across restarts and bind it to NodeId.
3. **Committee anchoring**: store/verify committee from program manifest; reject untrusted committees.
4. **Secure state sync**: stop applying unauthenticated `StateSyncMessage`; apply only finalized deltas/blocks.
5. **Consensus messages**: implement proposal/vote/QC types and networking.
6. **Finality**: implement HotStuff commit rule + catch-up sync.

