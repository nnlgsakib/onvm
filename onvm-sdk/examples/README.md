# ONVM SDK - Examples

This directory contains example usage of the ONVM SDK.

## Basic Usage Example

```typescript
import { OnvmClient, RpcMode } from 'onvm-sdk';
import * as fs from 'fs';

async function main() {
  // Create client in dev mode
  const client = new OnvmClient({
    rpcUrl: 'http://127.0.0.1:8080',
    mode: RpcMode.Dev,
  });

  // Check node health
  console.log('Checking node health...');
  const health = await client.health();
  console.log('Health:', health);

  // Deploy a WASM program
  console.log('\nDeploying program...');
  const wasmFile = fs.readFileSync('./echo.wasm');
  const wasmBase64 = wasmFile.toString('base64');

  const program = await client.deployProgram({
    wasm_base64: wasmBase64,
    entrypoint: 'onvm_main',
    blob_refs: [],
  });
  console.log('Program deployed:', program.id);

  // Execute the program
  console.log('\nExecuting program...');
  const input = Buffer.from('Hello, ONVM!').toString('base64');
  const result = await client.executeProgram({
    program_id: program.id,
    input_base64: input,
  });
  
  const output = Buffer.from(result.return_base64, 'base64').toString();
  console.log('Output:', output);
  console.log('Fuel consumed:', result.fuel);
}

main().catch(console.error);
```

## Authenticated Usage (Prod Mode)

```typescript
import { OnvmClient, RpcMode } from 'onvm-sdk';

async function main() {
  // Create client in prod mode with authentication
  const client = new OnvmClient({
    rpcUrl: 'https://api.onvm.network',
    mode: RpcMode.Prod,
    projectId: process.env.ONVM_PROJECT_ID!,
    projectSecret: process.env.ONVM_PROJECT_SECRET!,
    encryptResponses: true,
    timeout: 60000,
  });

  // All requests are automatically signed and encrypted
  const catalog = await client.listProgramCatalog();
  console.log('Programs:', catalog.catalog);
}

main().catch(console.error);
```

## Job Submission Example

```typescript
import { OnvmClient, RpcMode } from 'onvm-sdk';
import { randomBytes } from 'crypto';

async function submitAndMonitorJob() {
  const client = new OnvmClient({
    rpcUrl: 'http://127.0.0.1:8080',
    mode: RpcMode.Dev,
  });

  // Upload input data as blob
  const inputData = Buffer.from('Large input data...');
  const blob = await client.uploadBlob(inputData, 'application/octet-stream');
  console.log('Uploaded blob:', blob.id);

  // Submit job
  const job = await client.submitJob({
    request_id: randomBytes(16).toString('hex'),
    program_id: 'prog<64-hex>',
    input_blob_id: blob.id,
    max_retries: 3,
    metadata: {
      source: 'example-script',
    },
  });
  console.log('Job submitted:', job.job_id);

  // Poll for completion
  while (true) {
    const status = await client.getJobStatus(job.job_id);
    console.log('Job status:', status.status);

    if (status.status === 'completed') {
      const output = await client.getJobOutput(job.job_id);
      console.log('Job output:', output.output_base64);
      break;
    } else if (status.status === 'failed') {
      console.error('Job failed:', status.error_message);
      break;
    }

    await new Promise(resolve => setTimeout(resolve, 1000));
  }
}

submitAndMonitorJob().catch(console.error);
```

## Fuel Estimation Example

```typescript
import { OnvmClient, RpcMode } from 'onvm-sdk';

async function estimateExecutionCost() {
  const client = new OnvmClient({
    rpcUrl: 'http://127.0.0.1:8080',
    mode: RpcMode.Dev,
  });

  const programId = 'prog<64-hex>';
  const input = Buffer.from('test input').toString('base64');

  // Estimate fuel cost before execution
  const estimate = await client.estimateFuel({
    program_id: programId,
    input_base64: input,
  });

  console.log('Estimated fuel:', estimate.estimated_fuel);
  console.log('Confidence:', estimate.confidence);
  console.log('Based on samples:', estimate.based_on_samples);
  console.log('Estimated time:', estimate.estimated_execution_time_ms, 'ms');

  // Get detailed fuel profile
  const profile = await client.getFuelProfile(programId);
  console.log('\nFuel profile:');
  console.log('Average fuel/byte:', profile.average_fuel_per_byte);
  console.log('Base cost:', profile.base_fuel_cost);
  console.log('Samples:', profile.samples.length);
}

estimateExecutionCost().catch(console.error);
```

## Error Handling Example

```typescript
import { OnvmClient, RpcMode, OnvmError, OnvmAuthError, OnvmNetworkError } from 'onvm-sdk';

async function robustExecute() {
  const client = new OnvmClient({
    rpcUrl: 'http://127.0.0.1:8080',
    mode: RpcMode.Dev,
  });

  try {
    const result = await client.executeProgram({
      program_id: 'prog<64-hex>',
      input_base64: Buffer.from('input').toString('base64'),
    });
    console.log('Success:', result);
  } catch (error) {
    if (error instanceof OnvmAuthError) {
      console.error('Authentication failed:', error.message);
      console.error('Check your project ID and secret');
    } else if (error instanceof OnvmNetworkError) {
      console.error('Network error:', error.message);
      console.error('Is the node running?');
    } else if (error instanceof OnvmError) {
      console.error('ONVM error:', error.message);
      console.error('Status code:', error.statusCode);
      console.error('Details:', error.details);
    } else {
      console.error('Unexpected error:', error);
    }
    process.exit(1);
  }
}

robustExecute().catch(console.error);
```

## State Proof Verification Example

```typescript
import { OnvmClient, RpcMode } from 'onvm-sdk';

async function verifyStateProof() {
  const client = new OnvmClient({
    rpcUrl: 'http://127.0.0.1:8080',
    mode: RpcMode.Dev,
  });

  const programId = 'prog<64-hex>';
  const stateKey = Buffer.from('some-key').toString('hex');

  // Get state root with Merkle proof
  const stateRoot = await client.getProgramStateRoot(programId, stateKey);

  console.log('State root:', stateRoot.root);
  console.log('Value:', stateRoot.value_hex);
  console.log('Proof path:', stateRoot.proof);

  // The proof array contains hashes for Merkle verification
  if (stateRoot.proof && stateRoot.value_hex) {
    console.log('\nProof can be verified by:');
    console.log('1. Hash the value');
    console.log('2. Combine with proof hashes');
    console.log('3. Verify result matches state root');
  }
}

verifyStateProof().catch(console.error);
```
