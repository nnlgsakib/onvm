# ONVM SDK

Production-grade TypeScript/JavaScript SDK for interacting with ONVM RPC endpoints.

## Features

- ✅ Full RPC API coverage (health, projects, blobs, programs, jobs, fuel)
- ✅ Dev & Prod modes with automatic authentication
- ✅ HMAC-SHA256 request signing (matches Rust server implementation)
- ✅ XChaCha20Poly1305 response encryption/decryption
- ✅ Type-safe API with full TypeScript support
- ✅ Browser & Node.js compatible (using @stablelib for crypto)
- ✅ Configurable timeouts and comprehensive error handling

## Installation

```bash
npm install onvm-sdk @stablelib/xchacha20poly1305
# or
yarn add onvm-sdk @stablelib/xchacha20poly1305
```

## Quick Start

### Dev Mode (No Authentication)

```typescript
import { OnvmClient, RpcMode } from 'onvm-sdk';

const client = new OnvmClient({
  rpcUrl: 'http://127.0.0.1:8080',
  mode: RpcMode.Dev,
});

// Check health
const health = await client.health();
console.log(health);

// Deploy a program
const program = await client.deployProgram({
  wasm_base64: wasmBase64,
  entrypoint: 'onvm_main',
  blob_refs: [],
});
console.log('Program ID:', program.id);
```

### Prod Mode (With Authentication)

```typescript
import { OnvmClient, RpcMode } from 'onvm-sdk';

const client = new OnvmClient({
  rpcUrl: 'https://api.onvm.network',
  mode: RpcMode.Prod,
  projectId: 'your-project-id',
  projectSecret: 'your-32-byte-hex-secret',
  timeout: 30000, // 30 seconds
});

// All requests are automatically signed with HMAC-SHA256
// All responses are automatically decrypted using XChaCha20-Poly1305
const result = await client.executeProgram({
  program_id: 'program-id-hex',
  input_base64: inputBase64,
});
```

## API Reference

### Constructor

```typescript
new OnvmClient(config: OnvmClientConfig)
```

**Config Options:**
- `rpcUrl: string` - RPC endpoint URL
- `programId?: string` - Default program ID (optional)
- `projectId?: string` - Project ID (required in prod mode)
- `projectSecret?: string` - 32-byte hex secret (required in prod mode)
- `mode?: RpcMode` - `RpcMode.Dev` or `RpcMode.Prod` (default: Prod)
- `timeout?: number` - Request timeout in ms (default: 30000)

### Health Endpoints

```typescript
// Health check
await client.health(): Promise<HealthResponse>

// Liveness probe
await client.liveness(): Promise<LivenessResponse>

// Readiness probe with queue metrics
await client.readiness(): Promise<ReadinessResponse>

// Node metrics
await client.metrics(): Promise<MetricsResponse>
```

### Project Management

```typescript
// Create new project
await client.createProject({
  identity_passphrase: 'your-passphrase',
  project_id?: 'optional-project-id',
}): Promise<CreateProjectResponse>
```

### Blob Operations

```typescript
// Upload blob
await client.uploadBlob(
  data: Buffer | Uint8Array,
  mimeType?: string
): Promise<UploadBlobResponse>

// Download blob
await client.downloadBlob(blobId: string): Promise<Buffer>
```

### Program Operations

```typescript
// Deploy program
await client.deployProgram({
  wasm_base64: string,
  entrypoint: string,
  blob_refs: string[],
}): Promise<DeployProgramResponse>

// Get program info
await client.getProgramInfo(programId: string): Promise<ProgramInfo>

// Execute program
await client.executeProgram({
  program_id: string,
  input_base64: string,
}): Promise<ExecuteResponse>

// Get program receipts
await client.getProgramReceipts(programId: string): Promise<ProgramReceiptsResponse>

// Get committee info
await client.getProgramCommittee(programId: string): Promise<ProgramCommitteeResponse>

// Get state root (with optional Merkle proof)
await client.getProgramStateRoot(
  programId: string,
  key?: string
): Promise<ProgramStateRootResponse>

// List program catalog
await client.listProgramCatalog(): Promise<ProgramCatalogResponse>
```

### Job Operations

```typescript
// Submit job
await client.submitJob({
  request_id: string,
  program_id: string,
  input_base64?: string,
  input_blob_id?: string,
  max_retries?: number,
  metadata?: Record<string, string>,
}): Promise<SubmitJobResponse>

// Get job status
await client.getJobStatus(jobId: string): Promise<JobStatusResponse>

// Get job logs
await client.getJobLogs(jobId: string): Promise<JobLogsResponse>

// Get job output
await client.getJobOutput(jobId: string): Promise<JobOutputResponse>

// Cancel job
await client.cancelJob(jobId: string): Promise<void>

// List all jobs
await client.listJobs(): Promise<JobListResponse>
```

### Fuel Estimation

```typescript
// Estimate fuel cost
await client.estimateFuel({
  program_id: string,
  input_base64: string,
}): Promise<EstimateFuelResponse>

// Get fuel profile
await client.getFuelProfile(programId: string): Promise<FuelProfileResponse>
```

## Error Handling

```typescript
import { OnvmError, OnvmAuthError, OnvmNetworkError } from 'onvm-sdk';

try {
  await client.executeProgram({ ... });
} catch (error) {
  if (error instanceof OnvmAuthError) {
    console.error('Authentication failed:', error.message);
  } else if (error instanceof OnvmNetworkError) {
    console.error('Network error:', error.message);
  } else if (error instanceof OnvmError) {
    console.error('ONVM error:', error.message, error.statusCode);
  }
}
```

## Authentication Details

In **Prod mode**, the SDK automatically implements the same authentication protocol as the Rust server (`src/rpc/auth.rs`):

1. **Derives keys** using HKDF-SHA256:
   - Input: 32-byte project secret
   - Salt: None
   - Info: "onvm-rpc-auth"
   - Output: 64 bytes (32-byte signing_key + 32-byte response_key)

2. **Signs requests** using HMAC-SHA256:
   - Canonical format: `METHOD PATH\nTIMESTAMP\nNONCE\nBODY`
   - Headers added: `x-project-id`, `x-timestamp`, `x-nonce`, `x-signature`

3. **Decrypts responses** using XChaCha20-Poly1305:
   - Derives 24-byte nonce using HKDF with: salt=response_key, ikm=nonce, info="resp-{timestamp}"
   - Uses canonical request as AAD (Additional Authenticated Data)
   - Verifies authentication tag to prevent tampering

## Example: WASM Program with SDK

```typescript
// Inside your WASM program (JavaScript glue code)
export async function onvm_main(input: Uint8Array): Promise<Uint8Array> {
  const client = new OnvmClient({
    rpcUrl: 'http://localhost:8080',
    mode: RpcMode.Dev,
  });

  // Use the SDK inside your program
  const result = await client.executeProgram({
    program_id: 'some-program-id',
    input_base64: Buffer.from(input).toString('base64'),
  });

  return Buffer.from(result.return_base64, 'base64');
}
```

## Development

```bash
# Install dependencies
yarn install

# Build
yarn build

# Run tests
yarn test

# Lint
yarn lint
```

## License

MIT