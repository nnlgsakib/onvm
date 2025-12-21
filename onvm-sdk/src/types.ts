export enum RpcMode {
  Dev = 'dev',
  Prod = 'prod',
}

export interface OnvmClientConfig {
  rpcUrl: string;
  programId?: string;
  projectId?: string;
  projectSecret?: string;
  mode?: RpcMode;
  timeout?: number;
}

export interface HealthResponse {
  alive: boolean;
}

export interface LivenessResponse {
  alive: boolean;
}

export interface ReadinessResponse {
  status: string;
  queue_depth: number;
  running_count: number;
  capacity: number;
  available_slots: number;
}

export interface MetricsResponse {
  total_jobs: number;
  completed_jobs: number;
  failed_jobs: number;
  average_execution_time_ms: number;
  total_fuel_consumed: number;
}

export interface CreateProjectRequest {
  identity_passphrase: string;
  project_id?: string;
}

export interface CreateProjectResponse {
  project_id: string;
  project_secret: string;
}

export interface UploadBlobResponse {
  id: string;
  size: number;
}

export interface BlobInfo {
  id: string;
  size: number;
  mime_type?: string;
}

export interface DeployProgramRequest {
  wasm_base64: string;
  entrypoint: string;
  blob_refs: string[];
}

export interface DeployProgramResponse {
  id: string;
}

export interface ProgramInfo {
  id: string;
  publisher: string;
  size: number;
  entrypoint: string;
  blob_refs: string[];
}

export interface ExecuteRequest {
  program_id: string;
  input_base64: string;
}

export interface ExecuteResponse {
  return_base64: string;
  fuel: number;
}

export interface EstimateFuelRequest {
  program_id: string;
  input_base64: string;
}

export interface EstimateFuelResponse {
  estimated_fuel: number;
  confidence: number;
  based_on_samples: number;
  input_size_bytes: number;
  estimated_execution_time_ms: number;
}

export interface FuelSample {
  input_size: number;
  fuel_consumed: number;
  execution_time_ms: number;
  timestamp: number;
}

export interface FuelProfileResponse {
  program_id: string;
  samples: FuelSample[];
  average_fuel_per_byte: number;
  base_fuel_cost: number;
}

export interface SubmitJobRequest {
  request_id: string;
  program_id: string;
  input_base64?: string;
  input_blob_id?: string;
  max_retries?: number;
  metadata?: Record<string, string>;
}

export interface SubmitJobResponse {
  job_id: string;
  status: string;
}

export interface JobStatusResponse {
  job_id: string;
  request_id: string;
  program_id: string;
  status: string;
  fuel_consumed: number;
  created_at: number;
  started_at?: number;
  completed_at?: number;
  duration_ms?: number;
  retry_count: number;
  error_message?: string;
  metadata?: Record<string, string>;
}

export interface LogEntry {
  timestamp: number;
  level: string;
  message: string;
}

export interface JobLogsResponse {
  logs: LogEntry[];
}

export interface JobOutputResponse {
  job_id: string;
  output_base64: string;
  size: number;
}

export interface JobSummary {
  job_id: string;
  request_id: string;
  program_id: string;
  status: string;
  created_at: number;
  completed_at?: number;
}

export interface JobListResponse {
  jobs: JobSummary[];
  total: number;
}

export interface AggregatedReceipt {
  program_id: string;
  state_root_in: string;
  state_root_out: string;
  write_digest: string;
  gas_used: number;
  signer_bitmap: string;
  committee_epoch: number;
  receipt_id: string;
}

export interface ProgramReceiptsResponse {
  receipts: AggregatedReceipt[];
}

export interface CommitteeMember {
  node: string;
  weight: number;
  bls_public_key: string;
}

export interface ProgramCommitteeResponse {
  program_id: string;
  epoch: number;
  aggregate_public_key: string;
  members: CommitteeMember[];
  threshold: number;
}

export interface ProgramStateRootResponse {
  program_id: string;
  root: string;
  proof?: string[];
  value_hex?: string;
}

export interface CatalogEntry {
  program_id: string;
  version: number;
  initial_state_root: string;
  dag_parent?: string;
  code_manifest?: string;
  timestamp_ms: number;
}

export interface ProgramCatalogResponse {
  catalog: CatalogEntry[];
}