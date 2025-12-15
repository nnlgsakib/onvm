# ONVM Roadmap

A staged plan to evolve the ONVM ecosystem into two fully independent protocols:

1. **ONVM (Compute Protocol)** - Standalone distributed serverless WASM compute network
   - Binary: `onvm`
   - Runs completely independently, provides free computation
   - No blockchain dependency, no payment model built-in
   - Exposes RPC for job execution and resource metrics
   
2. **ONVM Chain (Blockchain Protocol)** - Independent DAG-based blockchain with native coin
   - Binary: `onvm-chain`
   - Runs as standalone blockchain network with consensus, accounts, and transactions
   - ONVM is the exclusive compute backend - no other VMs supported (no AWS Lambda, no alternatives)
   - Two types of participants:
     - **Validators**: Run chain consensus, validate transactions, earn base consensus rewards
     - **Compute Providers**: Run ONVM nodes, register with chain, execute jobs, earn fuel-based rewards
   - Validators and Providers are separate roles (can be same entity, but distinct functions)

**Key Architecture Flow:**
1. User submits compute operation to ONVM Chain
2. Chain queries registered ONVM Compute Providers' resource metrics via RPC
3. Chain intelligently routes task to optimal ONVM Provider
4. ONVM Provider executes job, returns output + fuel used
5. Chain pays Provider in native coins proportional to fuel consumed
6. Chain validators process transaction, validate execution receipt, earn consensus rewards

## Foundation (existing)
- [x] CLI entrypoints for init/run-node/upload-blob/deploy/execute/get-blob/program-info (`src/main.rs`, `src/cli.rs`)
- [x] Core runtime modules for networking, consensus, execution, storage, RPC, crypto, block assembly (`src/node.rs`, submodules)
- [x] Shared types/config defaults in `src/types.rs`, `src/config`
- [x] Sample WASM programs under `wasm_programs/`
- [ ] Separate binary targets: 
  - `onvm` (pure compute node - runs standalone)
  - `onvm-chain` (blockchain validator node - consensus only)
- [ ] Modular architecture: 
  - ONVM has zero blockchain code
  - ONVM Chain has ONVM RPC client module (exclusively for ONVM, no other backends)

---

## ONVM Protocol (Pure Compute Network)

**Core Principle:** ONVM is a pure compute protocol. It knows nothing about payments, subscriptions, or blockchain. It only executes WASM jobs and returns results.

### Phase 1: Pure Compute Foundation (0–2 milestones)
- [x] Define job/function model and manifest (resources, capabilities, I/O schemas), content-addressed blobs, versioning
- [x] RPC surface for submit-job, get-status, fetch-output/logs, cancel; idempotent request IDs
- [x] WASM sandbox hardening: fuel metering, timeouts, memory/malloc limits, allowed imports; signature verification on modules
- [x] Node health reporting and metrics: liveness/ready endpoints, capacity metrics (CPU/mem/queue), tracing spans
- [x] Network coordination: peer discovery, capability broadcasting, load balancing
- [ ] Advanced scheduler: pick nodes by capacity/health; include retry/backoff, job TTLs, failure reasons
- [ ] CLI/SDK ergonomics: package+upload function, submit job, stream status/logs, local-run parity
- [ ] Integration tests for job lifecycle and sandbox enforcement
- [ ] Resource tracking: measure CPU/mem/fuel/storage used per job (metrics only, no payment logic)

### Phase 2: Execution Optimization (2–4 milestones)
- [ ] Redundant execution (N-of-M) with majority/threshold validation for critical workloads
- [ ] Execution receipts with commitments (inputs/outputs/resource usage) and audit logs
- [ ] Result caching and memoization for deterministic functions (shared cache across nodes)
- [ ] Geo/latency-aware scheduling and data locality hints
- [ ] Multi-tenancy guardrails: per-tenant quotas, isolation policies, noisy-neighbor protections
- [ ] Reputation scoring (success rate, latency, availability) - local/trust-based, no payment implications
- [ ] Enhanced peer discovery via gossip/DHT; NAT traversal; normalized multiaddrs
- [ ] Computation metrics API: expose CPU/mem/fuel/storage usage per job (consumed by external systems)

### Phase 2.5: Advanced P2P Networking Layer (2.5–4 milestones)

**Multi-Protocol Transport Support:**
- [ ] Protocol abstraction layer: pluggable transport implementations
- [ ] TCP transport: reliable, connection-oriented baseline (existing)
- [ ] QUIC transport: UDP-based, multiplexed streams, built-in encryption, faster connection establishment
  - 0-RTT connection resumption for reduced latency
  - Connection migration support (mobile/dynamic IP scenarios)
  - Per-stream flow control and prioritization
- [ ] UDP transport: for low-latency, lossy-tolerant messages (heartbeats, metrics broadcasts)
- [ ] WebSocket transport: browser-compatible, firewall-friendly
- [ ] WebTransport: modern browser API over HTTP/3, bidirectional streams
- [ ] Protocol negotiation: automatic selection based on peer capabilities and network conditions
- [ ] Fallback chain: QUIC → TCP → WebSocket (try fastest, fall back to most compatible)

**Advanced Peer Discovery:**
- [ ] Multi-strategy peer discovery:
  - mDNS (existing): local network discovery
  - DHT-based discovery: Kademlia for global peer routing
  - Rendezvous protocol: centralized discovery servers for bootstrapping
  - Bootstrap nodes: hardcoded seed nodes for initial network entry
  - Peer exchange (PEX): learn peers from connected nodes
- [ ] Peer scoring and ranking:
  - Score based on: latency, bandwidth, uptime, protocol support, reliability
  - Prioritize high-quality peers for critical connections
  - Blacklist/deprioritize misbehaving peers
- [ ] Capability-based discovery: find peers with specific features (GPU, high bandwidth, low latency)
- [ ] Geographic/regional discovery: prefer nearby peers for data locality
- [ ] Discovery caching: persist discovered peers across restarts

**Dialing & Connection Management:**
- [ ] Simultaneous multi-dial: attempt multiple transports in parallel, use fastest to connect
- [ ] Connection pooling: reuse existing connections, multiplex streams
- [ ] Dial backoff and retry: exponential backoff for failed dials, circuit breaker for persistent failures
- [ ] Adaptive dialing: learn which transports work best per peer, prioritize accordingly
- [ ] NAT traversal improvements:
  - STUN/TURN support for NAT hole-punching
  - UPnP/NAT-PMP for automatic port forwarding
  - Relay fallback when direct connection impossible
  - DCUtR (Direct Connection Upgrade through Relay): upgrade relay to direct connection
- [ ] Connection limits: max connections per peer, global connection limits, memory-based throttling
- [ ] Connection keep-alive: periodic ping/pong to detect dead connections
- [ ] Graceful connection closure: proper shutdown handshake, resource cleanup

**Gossip & Broadcast Optimization:**
- [ ] Efficient gossip protocols:
  - Epidemic broadcast: randomized peer selection for message propagation
  - Plumtree (Push-Lazy-Push Multicast Tree): hybrid eager/lazy push
  - Mesh-based gossip: maintain overlay mesh topology
- [ ] Message deduplication: bloom filters, LRU cache to prevent re-broadcasting
- [ ] Gossip fanout tuning: adaptive fanout based on network size and churn
- [ ] Topic-based subscription: only gossip to interested peers
- [ ] Priority-based propagation: critical messages (consensus events) get priority over data blobs

**Network Observability & Diagnostics:**
- [ ] Per-transport metrics: bytes sent/received, connection count, error rates
- [ ] Peer metrics: latency histograms, bandwidth utilization, protocol versions
- [ ] Network topology visualization: export connection graph for monitoring
- [ ] Diagnostic RPC endpoints: query peer list, connection states, transport stats
- [ ] Event logging: connection/disconnection events, protocol upgrades, errors
- [ ] Performance profiling: identify slow peers, congested transports, bottlenecks

### Phase 3: GPU & High-Performance Execution (4–6 milestones)
- [ ] GPU capability discovery: enumerate GPU model/VRAM/compute capability, driver/runtime versions
- [ ] GPU-aware manifests: request GPU type/count, memory, precision (fp16/fp32), kernel/runtime requirements
- [ ] GPU sandboxing and isolation: container/runtime profiles, job-level quotas and preemption
- [ ] GPU scheduling: binpack or priority scheduling based on GPU fit, VRAM fragmentation awareness, placement constraints
- [ ] Data path for large models: staged uploads, caching, and streaming of model shards to GPU nodes
- [ ] Validation hooks for AI jobs: checksum/attestation of model blobs, deterministic seeds where required

### Phase 4: Security, UX & Ecosystem (6–7 milestones)
- [ ] Security reviews and fuzzing for host interfaces, RPC, job parsing; admission control defaults deny-all
- [ ] Public function registry: publish/browse functions, usage analytics, curated examples
- [ ] Privacy enhancements for inputs/outputs (encryption, access tokens, redaction)
- [ ] Protocol versioning and compatibility gates; rolling upgrade playbook and feature flags
- [ ] Documentation for running ONVM nodes, deploying functions, network participation
- [ ] ONVM remains pure compute - no payment, subscription, or blockchain concepts

---

## ONVM Chain Protocol (Standalone Blockchain)

**Core Principle:** ONVM Chain runs as a completely independent blockchain. No ONVM compute dependency by default. Validators run consensus and earn rewards. Optionally, validators can connect to ONVM nodes via `--onvm-rpc` flag to enable paid computation.

### Phase 1: Hashgraph Consensus Foundation (0–2 milestones)
- [ ] Hashgraph consensus implementation: gossip about gossip protocol, virtual voting, consensus timestamps
- [ ] Event structure and DAG ordering: witness events, famous witness determination, consensus ordering without blocks
- [ ] Byzantine fault tolerance: handle up to 1/3 malicious nodes, ensure fairness and fast finality
- [ ] Standalone chain network: peer discovery and membership via gossip/DHT; NAT traversal
- [ ] Observability stack: metrics exporter, log shipping, dashboards, alerts for consensus health (round creation, famous witnesses, event throughput)
- [ ] Chain runs independently - no ONVM compute integration by default

### Phase 2: Native Coin, Accounts & Subscriptions (2–4 milestones)
- [ ] Design native coin economics: supply/emission, mint/burn rules, tokenomics (independent of compute)
- [ ] Account model and transactions: balances, nonces, signatures, replay protection, key management
- [ ] Subscription model: users purchase compute subscriptions (time-based or usage-based credits)
- [ ] Subscription transactions: buy subscription, renew, cancel, query remaining credits
- [ ] Transaction pool and ordering: fee prioritization via consensus timestamps, DoS protection, rate limiting
- [ ] DAG transaction integration: consensus ordering via hashgraph, balance updates, subscription state, state proofs
- [ ] Wallet/SDK support: create/import accounts, sign transactions, view balances/subscriptions, transfers
- [ ] Tests for accounting correctness, subscription logic, overflow/underflow guards, signature validation

### Phase 3: Validator & Provider Architecture (4–6 milestones)

**Two Distinct Roles:**

#### A. Chain Validators (Consensus Layer)
- [ ] Staking mechanism for validators: collateral requirements, delegation support
- [ ] Validator responsibilities:
  - Run DAG consensus (gossip, virtual voting, ordering)
  - Validate compute operation transactions
  - Verify execution receipts from Providers
  - Maintain chain state (accounts, balances, provider registry)
- [ ] Validator rewards: round rewards, transaction fees, treasury allocation
- [ ] Validator slashing: Byzantine behavior, downtime, incorrect receipt validation
- [ ] Full stake-weighted voting and dynamic validator membership
- [ ] Validators DO NOT execute compute - they only validate the chain

#### B. Compute Providers (Execution Layer)
- [ ] Provider registration on-chain:
  - Provider runs ONVM node: `onvm run-node`
  - Provider registers on chain: `RegisterProvider { onvm_endpoint, capabilities, stake_amount, min_fuel_price }`
  - Chain maintains provider registry in state
  - Providers must stake coins to participate (prevent spam/malicious nodes)
- [ ] Provider capabilities:
  - CPU cores, RAM, GPU model/VRAM, storage capacity
  - Specialized features: AI/ML optimized, high-memory, GPU compute
  - Availability schedule, geographic region, max concurrent jobs
- [ ] Provider rewards:
  - ONVM Provider executes job, returns output + fuel_consumed
  - Chain calculates: `reward = fuel_consumed × fuel_price`
  - Chain pays Provider directly from job payment escrow
  - Immediate settlement after execution receipt validation by validators
- [ ] Provider slashing:
  - False fuel reporting (claims more fuel than actually used)
  - Job execution failure/timeout
  - Incorrect results (detected via redundant execution)
  - Providers lose staked coins for misbehavior
- [ ] Provider exit: can unstake and deregister after cooldown period

### Phase 4: Advanced Features & Scaling (6–8 milestones)
- [ ] State snapshots and fast sync: periodic state commitments in DAG for quick node bootstrap
- [ ] Advanced fee markets with dynamic pricing based on consensus round congestion
- [ ] On-chain governance: parameter updates, protocol upgrades, treasury management

---

## ONVM Chain Integration with ONVM (Chain-Side Only)

**Core Principle:** Integration happens ONLY in ONVM Chain code. ONVM binary has ZERO chain code. ONVM is the ONLY compute backend - no AWS, no alternatives.

### Phase 1: Provider Registry & ONVM RPC Client (5–7 milestones)
- [ ] ONVM RPC client library in ONVM Chain codebase:
  - Exclusively designed for ONVM protocol
  - Consumes ONVM's existing RPC endpoints
  - No support for other compute backends (AWS/GCP/Azure)
- [ ] Resource metrics API in ONVM (standardized endpoints):
  - `/metrics` endpoint: CPU%, memory%, queue depth, active jobs, GPU status
  - `/health` endpoint: node status, capacity, availability, version
  - `/capabilities` endpoint: hardware specs, supported features
  - ONVM just exposes data, no chain awareness
- [ ] Provider registry in chain state:
  - On-chain transaction: `RegisterProvider { onvm_endpoint, capabilities, stake, min_fuel_price }`
  - Chain validates provider by querying ONVM `/health` endpoint
  - Chain stores: provider_id, onvm_endpoint, capabilities, stake_amount, registration_block, status
  - Providers can update capabilities, min_fuel_price, or deregister
- [ ] Compute operation transactions in chain:
  - New transaction type: `SubmitComputeOp { program_id, input, max_fuel, fuel_price }`
  - User pays upfront: `payment = max_fuel × fuel_price` (locked in escrow)
  - Chain validates: sufficient balance, valid program_id, fuel_price meets market minimum
- [ ] Chain validators query provider registry:
  - Validators don't execute compute themselves
  - Validators only route jobs to registered ONVM Providers
  - Validators verify execution receipts from Providers
- [ ] No changes to ONVM binary - it responds to standard RPC requests

### Phase 2: Intelligent Routing & Fuel-Based Rewards (7–9 milestones)

**Job Routing Architecture:**

- [ ] Routing algorithm in chain consensus:
  - When user submits `SubmitComputeOp`, chain selects Provider via routing algorithm
  - Algorithm considers:
    - Provider capabilities (CPU/GPU) vs job requirements
    - Current resource utilization (from `/metrics`)
    - Provider reputation score (success rate, uptime)
    - Geographic latency (if available)
    - Provider's `min_fuel_price` vs user's `fuel_price`
  - Chain selects top-scoring Provider, assigns job
  - If Provider rejects/fails, chain tries next Provider in ranking

**Execution Flow (Validator ↔ Provider Separation):**

1. **User submission:**
   - User submits `SubmitComputeOp` tx to chain
   - Transaction includes: program_id, input, max_fuel, fuel_price
   - User pays: `payment = max_fuel × fuel_price` (locked in escrow in chain state)

2. **Validator processes transaction:**
   - Chain validators validate tx via consensus
   - Consensus selects optimal ONVM Provider from registry
   - Chain state records: job_id, assigned_provider, escrow_amount, timestamp

3. **Provider receives job assignment:**
   - Assigned Provider queries chain state for pending jobs (pull model)
   - OR Chain broadcasts job assignment event (push model via separate protocol)
   - Provider fetches job details from chain

4. **Provider executes on ONVM:**
   - Provider calls local ONVM node: `POST /jobs { program_id, input }`
   - ONVM executes (unaware of chain/payment)
   - ONVM returns: `{ output, fuel_consumed, state_root }`

5. **Provider submits execution receipt to chain:**
   - Provider submits on-chain tx: `SubmitExecutionReceipt { job_id, output, fuel_consumed, signature }`
   - Receipt includes ONVM's returned data + Provider's signature

6. **Validators verify receipt:**
   - Chain validators verify receipt signature
   - Optionally: trigger redundant execution by another Provider to verify correctness
   - Consensus validates receipt and finalizes job

7. **Payment settlement:**
   - Chain calculates: `actual_reward = fuel_consumed × fuel_price`
   - Chain pays Provider from escrow
   - Chain refunds user if `fuel_consumed < max_fuel`
   - Chain records payment in state

**Key Features:**
- [ ] Execution receipts stored in chain state (immutable audit log)
- [ ] Timeout handling: if Provider doesn't submit receipt within deadline, job marked failed, escrow refunded
- [ ] Provider reputation tracking: success rate, average fuel accuracy, response time
- [ ] Fuel price market: Providers set min_fuel_price, users set max fuel_price, chain matches
- [ ] ONVM binary remains pure compute - no chain dependencies

### Phase 3: Advanced Features & Verification (9–11 milestones)

**Redundant Execution & Verification:**

- [ ] Multi-Provider execution for critical jobs:
  - User can request redundant execution: `SubmitComputeOp { ..., redundancy_level: 3 }`
  - Chain assigns job to N Providers (e.g., 3)
  - All N Providers execute same job on their ONVM nodes independently
  - All N Providers submit execution receipts to chain
  - Chain consensus compares results:
    - Majority result accepted as correct
    - Providers in majority get full reward
    - Providers in minority get slashed (incorrect execution penalty)
  - Ensures Byzantine fault tolerance at execution layer

**Provider Reputation & Performance Tracking:**

- [ ] Reputation system in chain state:
  - Track per-Provider metrics: success_rate, avg_fuel_accuracy, uptime, total_jobs, failed_jobs
  - Calculate reputation score: `score = f(success_rate, fuel_accuracy, uptime, stake_amount)`
  - Higher reputation → higher routing priority
  - Reputation decays over time if Provider inactive
- [ ] Performance-based routing:
  - Boost jobs to high-reputation Providers
  - New Providers start with neutral reputation, build up via successful executions
  - Providers with low reputation (<threshold) automatically suspended from routing
- [ ] Provider suspension/ejection:
  - Automatic suspension if: reputation < min_threshold OR slashed > max_slash_count
  - Providers can appeal suspension via on-chain governance
  - Ejected Providers lose stake, cannot re-register for cooldown period

**Marketplace & Discovery:**

- [ ] On-chain program registry:
  - Publishers submit: `RegisterProgram { wasm_hash, metadata, recommended_fuel_budget, required_capabilities }`
  - Chain stores program metadata (not WASM itself - WASM lives in ONVM blob store)
  - Users browse programs via chain state or indexer
- [ ] Job requirement matching:
  - Programs specify requirements: `{ min_cpu_cores: 4, requires_gpu: true, min_ram_gb: 8 }`
  - Chain routing only considers Providers that meet requirements
  - GPU jobs → GPU Providers, high-memory jobs → high-RAM Providers
- [ ] Analytics & monitoring:
  - Chain state tracks: total_compute_jobs, total_fuel_consumed, total_rewards_paid
  - Per-Provider leaderboard: top earners, most reliable, fastest
  - Per-program stats: avg_fuel_used, success_rate, popularity
- [ ] Load balancing improvements:
  - Distribute jobs to prevent Provider overload
  - Detect Provider congestion via `/metrics`, deprioritize overloaded Providers
  - Round-robin among equally-ranked Providers to spread load

**No External Dependencies:**
- [ ] ONVM is the exclusive compute layer - no external VM support
- [ ] All compute verifiability via redundant execution
- [ ] Chain-ONVM integration is permanent and exclusive
- [ ] All integration logic in onvm-chain binary, onvm binary remains pure

---

## Exploratory / Long-Term

### ONVM Protocol (Pure Compute)
- [ ] WebAssembly Component Model support for polyglot functions
- [ ] Streaming/reactive job types (long-running workers, event-driven triggers)
- [ ] Edge deployment: lightweight ONVM nodes on IoT/mobile devices
- [ ] Federated learning and privacy-preserving computation patterns
- [ ] ONVM remains payment-agnostic - exposes RPC for external systems

### ONVM Chain Protocol (Blockchain + ONVM Compute)
- [ ] Smart contract VM for general-purpose programmable logic
- [ ] Sharding or layer-2 solutions for extreme blockchain scale
- [ ] Cross-chain bridges to external blockchains for liquidity
- [ ] Decentralized identity and credential verification tied to chain accounts
- [ ] On-chain governance for chain parameters, fuel price floors/caps, Provider requirements
- [ ] DeFi primitives: DEX, lending, staking derivatives
- [ ] ONVM remains the exclusive compute layer

### ONVM Chain Economic Features (Chain-Side Only)

**Dynamic Fuel Market:**

- [ ] Market-based fuel pricing:
  - Providers set `min_fuel_price` in their registry entry
  - Users set `fuel_price` in job submission (must meet or exceed Provider's minimum)
  - Chain matches users with willing Providers
  - High demand → Providers raise min_fuel_price (market-driven scarcity pricing)
  - Low demand → Providers lower min_fuel_price to attract jobs
- [ ] Price discovery mechanism:
  - Chain tracks: avg_fuel_price, median_fuel_price, price_trend
  - Recommended fuel_price shown to users based on recent market activity
  - No AMM or complex DeFi - simple supply/demand matching

**Compute Reservations:**

- [ ] Advance capacity commitment:
  - Users submit: `ReserveCapacity { duration_blocks, max_jobs, fuel_price_locked }`
  - Chain locks payment upfront: `payment = estimated_fuel × fuel_price_locked`
  - Providers opt-in to serve reserved capacity
  - Providers get guaranteed revenue, users get price stability
- [ ] Reservation priority:
  - Reserved jobs routed before non-reserved jobs
  - Providers honor reservations or face slashing
  - Unused reservation capacity refunded after expiry

**Payment Optimization:**

- [ ] Batch settlement for frequent users:
  - High-volume users can open escrow account on chain
  - Submit multiple jobs without per-job payment tx
  - Chain deducts from escrow balance
  - Settle/top-up escrow periodically to reduce tx overhead
- [ ] Slashing fund redistribution:
  - Coins slashed from dishonest Providers go to treasury
  - Treasury redistributes to: honest Providers, validators, burn (per governance)
  - Incentivizes honest behavior, penalizes fraud

**Architecture Constraints:**

- [ ] ONVM is the ONLY compute backend:
  - Chain exclusively designed for ONVM protocol
  - Tight integration between chain and ONVM via dedicated RPC client
- [ ] Execution verification via redundant execution:
  - Multiple Providers run same job independently
  - Consensus compares results, majority wins
  - Simpler and more transparent verification model
- [ ] All integration code in onvm-chain binary:
  - onvm binary remains pure compute
  - Chain handles all payment, routing, verification logic