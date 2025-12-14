# Job Test Program - Testing Guide

## Overview
This WASM program demonstrates the new job execution system with various task types that test different aspects of the scheduler, retry logic, and resource management.

## Build the WASM Program

```bash
cd wasm_programs/job-test
cargo build --target wasm32-unknown-unknown --release
cd ../..
```

## Start the Node

```bash
# Terminal 1: Start the node
cargo run -- init --data-dir ./node1
cargo run -- run-node --data-dir ./node1 --rpc 127.0.0.1:8080
```

## Deploy the Program

```bash
# Terminal 2: Deploy the job-test program
cargo run -- deploy \
  --rpc 127.0.0.1:8080 \
  --file wasm_programs/job-test/target/wasm32-unknown-unknown/release/job_test.wasm \
  --entrypoint onvm_main

# Save the returned program_id for later use
# Example output: {"id":"abc123..."}
```

## Submit Jobs

### 1. Compute Task (Math Operations)
```bash
cargo run -- submit-job \
  --rpc 127.0.0.1:8080 \
  --program-id <PROGRAM_ID> \
  --input wasm_programs/job-test/test_compute.json \
  --request-id compute-test-1 \
  --max-retries 3

# Save the job_id from response
```

### 2. Hash Task (Iterative Hashing)
```bash
cargo run -- submit-job \
  --rpc 127.0.0.1:8080 \
  --program-id <PROGRAM_ID> \
  --input wasm_programs/job-test/test_hash.json \
  --request-id hash-test-1 \
  --max-retries 2
```

### 3. Transform Task (String Transformation)
```bash
cargo run -- submit-job \
  --rpc 127.0.0.1:8080 \
  --program-id <PROGRAM_ID> \
  --input wasm_programs/job-test/test_transform.json \
  --request-id transform-test-1
```

### 4. Stress Task (Heavy Computation)
```bash
cargo run -- submit-job \
  --rpc 127.0.0.1:8080 \
  --program-id <PROGRAM_ID> \
  --input wasm_programs/job-test/test_stress.json \
  --request-id stress-test-1 \
  --max-retries 3
```

## Monitor Jobs

### Check Job Status
```bash
cargo run -- job-status \
  --rpc 127.0.0.1:8080 \
  --job-id <JOB_ID>
```

### View Job Logs
```bash
cargo run -- job-logs \
  --rpc 127.0.0.1:8080 \
  --job-id <JOB_ID>
```

### Get Job Output
```bash
cargo run -- job-output \
  --rpc 127.0.0.1:8080 \
  --job-id <JOB_ID> \
  --out ./output.json

# View the output
cat output.json
```

### List All Jobs
```bash
cargo run -- list-jobs --rpc 127.0.0.1:8080
```

### Cancel a Job
```bash
cargo run -- cancel-job \
  --rpc 127.0.0.1:8080 \
  --job-id <JOB_ID>
```

## Using RPC Endpoints Directly

### Submit Job via curl
```bash
curl -X POST http://127.0.0.1:8080/jobs \
  -H "Content-Type: application/json" \
  -d '{
    "request_id": "curl-test-1",
    "program_id": "<PROGRAM_ID>",
    "input_base64": "eyJ0YXNrIjoiY29tcHV0ZSIsIml0ZXJhdGlvbnMiOjEwMCwiZGF0YSI6IjQyIn0=",
    "max_retries": 3
  }'
```

### Get Job Status via curl
```bash
curl http://127.0.0.1:8080/jobs/<JOB_ID>
```

### Get Job Logs via curl
```bash
curl http://127.0.0.1:8080/jobs/<JOB_ID>/logs
```

### Get Job Output via curl
```bash
curl http://127.0.0.1:8080/jobs/<JOB_ID>/output | jq -r '.output_base64' | base64 -d
```

### List All Jobs via curl
```bash
curl http://127.0.0.1:8080/jobs
```

### Cancel Job via curl
```bash
curl -X POST http://127.0.0.1:8080/jobs/<JOB_ID>/cancel
```

## Check Node Health

### Liveness
```bash
curl http://127.0.0.1:8080/health/liveness
```

### Readiness
```bash
curl http://127.0.0.1:8080/health/readiness
```

### Metrics
```bash
curl http://127.0.0.1:8080/health/metrics
```

## Testing Scenarios

### Test Idempotency (Same request_id)
```bash
# Submit same request_id twice
cargo run -- submit-job --rpc 127.0.0.1:8080 --program-id <PROGRAM_ID> --input wasm_programs/job-test/test_compute.json --request-id idempotent-test-1

# Second submission with same request_id should return the same job
cargo run -- submit-job --rpc 127.0.0.1:8080 --program-id <PROGRAM_ID> --input wasm_programs/job-test/test_compute.json --request-id idempotent-test-1
```

### Test Retry Logic (Submit invalid program)
```bash
# This will fail and retry
cargo run -- submit-job \
  --rpc 127.0.0.1:8080 \
  --program-id 0000000000000000000000000000000000000000000000000000000000000000 \
  --input wasm_programs/job-test/test_compute.json \
  --request-id retry-test-1 \
  --max-retries 3

# Watch the retry attempts
cargo run -- job-logs --rpc 127.0.0.1:8080 --job-id <JOB_ID>
```

### Test Parallel Execution
```bash
# Submit multiple jobs simultaneously
for i in {1..5}; do
  cargo run -- submit-job \
    --rpc 127.0.0.1:8080 \
    --program-id <PROGRAM_ID> \
    --input wasm_programs/job-test/test_compute.json \
    --request-id parallel-test-$i &
done
wait

# Check all jobs
cargo run -- list-jobs --rpc 127.0.0.1:8080
```

## Expected Outputs

### Compute Task
```json
{
  "status": "success",
  "result": "computed result: 123456789",
  "iterations_completed": 100,
  "fuel_estimate": "~110000 fuel units"
}
```

### Hash Task
```json
{
  "status": "success",
  "result": "hash: a1b2c3d4e5f6g7h8",
  "iterations_completed": 50,
  "fuel_estimate": "~60000 fuel units"
}
```

### Transform Task
```json
{
  "status": "success",
  "result": "transformed: TeSt tHe jOb sChEdUlEr wItH ThIs mEsSaGe",
  "iterations_completed": 10,
  "fuel_estimate": "~20000 fuel units"
}
```

### Stress Task
```json
{
  "status": "success",
  "result": "stress test completed: 500 iterations, sum: 12345",
  "iterations_completed": 500,
  "fuel_estimate": "~510000 fuel units"
}
```
