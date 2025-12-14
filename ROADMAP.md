# ONVM Roadmap

A staged plan to evolve ONVM into a distributed, crypto-powered serverless compute network with per-operation payments, subscriptions, and node rewards. Built on a DAG-based hashgraph consensus (inspired by Hedera Hashgraph) for high throughput and fast finality - no traditional blocks.

## Foundation (existing)
- [x] CLI entrypoints for init/run-node/upload-blob/deploy/execute/get-blob/program-info (`src/main.rs`, `src/cli.rs`)
- [x] Core runtime modules for networking, consensus, execution, storage, RPC, crypto, block assembly (`src/node.rs`, submodules)
- [x] Shared types/config defaults in `src/types.rs`, `src/config`
- [x] Sample WASM programs under `wasm_programs/`

## Phase 1: Job & Runtime Base (0–2 milestones)
- [x] Define job/function model and manifest (resources, capabilities, I/O schemas), content-addressed blobs, versioning
- [x] RPC surface for submit-job, get-status, fetch-output/logs, cancel; idempotent request IDs
- [x] WASM sandbox hardening: fuel metering, timeouts, memory/malloc limits, allowed imports; signature verification on modules
- [x] Node health reporting and metrics: liveness/ready endpoints, capacity metrics (CPU/mem/queue), tracing spans
- [ ] advanced scheduler: pick nodes by capacity/health; include retry/backoff, job TTLs, failure reasons
- [ ] CLI/SDK ergonomics: package+upload function, submit job, stream status/logs, local-run parity
- [ ] Integration tests for job lifecycle and sandbox enforcement

## Phase 2: Native Coin & Accounting (1–3 milestones)
- [ ] Design native coin economics: supply/emission, fee model, staking needs, mint/burn rules
- [ ] Account model and transactions: balances, nonces, signatures, replay protection, key management
- [ ] Per-operation metering/pricing: translate resource usage (CPU/mem/fuel/storage) into coin fees; tiered subscription allowances with overage billing
- [ ] Fee collection pipeline: attach payments to job submissions, hold escrow until completion/cancellation, refund rules
- [ ] DAG transaction integration: transaction pool, consensus ordering via hashgraph, execution receipts in DAG events, balance updates, state proofs
- [ ] Wallet/SDK support: create/import accounts, sign transactions, view balances/allowances
- [ ] Tests for accounting correctness, overflow/underflow guards, and signature validation

## Phase 3: Incentives & Settlements (3–5 milestones)
- [ ] Provider rewards: distribute fees to executing nodes based on measured usage; include treasury cut/burn if applicable
- [ ] Staking and slashing: collateral requirements for executors; slash on proven misbehavior or chronic failures
- [ ] Execution receipts with commitments (inputs/outputs/resource usage) and audit logs; dispute and re-execution flow
- [ ] Redundant execution (N-of-M) with majority/threshold validation; tie rewards/penalties to agreement results
- [ ] Reputation scoring that feeds scheduling (success rate, latency, dispute history, decay)
- [ ] Payout and settlement cadence: consensus-round payouts, epochs, or streaming payments; handle dust limits and batching

## Phase 4: Hashgraph Consensus & Network Hardening (5–6 milestones)
- [ ] Hashgraph consensus implementation: gossip about gossip protocol, virtual voting, consensus timestamps
- [ ] Event structure and DAG ordering: witness events, famous witness determination, consensus ordering without blocks
- [ ] Byzantine fault tolerance: handle up to 1/3 malicious nodes, ensure fairness and fast finality
- [ ] Peer discovery and membership via gossip/DHT; NAT traversal; normalized multiaddrs
- [ ] Transaction pool and ordering: fee prioritization via consensus timestamps, subscription allowance checks, DoS protection, rate limiting
- [ ] Protocol versioning and compatibility gates; rolling upgrade playbook and feature flags
- [ ] Observability stack: metrics exporter, log shipping, dashboards, alerts for consensus health (round creation, famous witnesses, transaction throughput)

## Phase 5: Security, UX, and Marketplace (6–7 milestones)
- [ ] Security reviews and fuzzing for host interfaces, RPC, transaction parsing; admission control defaults deny-all
- [ ] Multi-tenancy guardrails: per-tenant quotas, isolation policies, subscription tiers, noisy-neighbor protections
- [ ] Geo/latency-aware scheduling and data locality hints
- [ ] Public marketplace UX: publish/browse functions, usage analytics, curated examples; publisher verification
- [ ] Compliance/backup/SLOs for managed offerings; incident runbooks and key-rotation procedures

## Phase 6: GPU & High-Performance Execution (7–8 milestones)
- [ ] GPU capability discovery: enumerate GPU model/VRAM/compute capability, driver/runtime versions; expose in node metadata
- [ ] GPU-aware manifests: request GPU type/count, memory, precision (fp16/fp32), kernel/runtime requirements (CUDA/ROCm/WebGPU)
- [ ] GPU sandboxing and isolation: container/runtime profiles, cgroups/driver isolation, job-level quotas and preemption
- [ ] GPU scheduling: binpack or priority scheduling based on GPU fit, VRAM fragmentation awareness, placement constraints
- [ ] GPU metering and billing: track GPU time/SM usage/VRAM; pricing that feeds per-op fees and subscriptions
- [ ] Data path for large models: staged uploads, caching, and streaming of model shards to GPU nodes; eviction policies
- [ ] Validation hooks for AI jobs: checksum/attestation of model blobs, deterministic seeds where required

## Exploratory / Long-Term
- [ ] Full Hashgraph consensus with stake-weighted voting and dynamic membership
- [ ] Optional TEE or zk-assisted execution proofs for high-assurance verification
- [ ] Hybrid settlement/bridging to external chains for liquidity and cross-chain payments
- [ ] Advanced fee markets with dynamic pricing based on consensus round congestion
- [ ] Privacy enhancements for inputs/outputs (encryption, access tokens, redaction)
- [ ] State snapshots and fast sync: periodic state commitments in DAG for quick node bootstrap