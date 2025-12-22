import {
  OnvmClientConfig,
  HealthResponse,
  LivenessResponse,
  ReadinessResponse,
  MetricsResponse,
  CreateProjectRequest,
  CreateProjectResponse,
  UploadBlobResponse,
  DeployProgramRequest,
  DeployProgramResponse,
  ProgramInfo,
  ExecuteRequest,
  ExecuteResponse,
  EstimateFuelRequest,
  EstimateFuelResponse,
  FuelProfileResponse,
  SubmitJobRequest,
  SubmitJobResponse,
  JobStatusResponse,
  JobLogsResponse,
  JobOutputResponse,
  JobListResponse,
  ProgramReceiptsResponse,
  ProgramCommitteeResponse,
  ProgramStateRootResponse,
  ProgramCatalogResponse,
} from './types';
import { OnvmError, OnvmNetworkError } from './errors';
import { deriveKeys, buildAuthHeaders, DerivedKeys } from './auth';

export class OnvmClient {
  private rpcUrl: string;
  private programId?: string;
  private projectId?: string;
  private projectSecret?: string;
  private timeout: number;
  private derivedKeys?: DerivedKeys;
  private detectedNodeMode?: 'dev' | 'prod';

  constructor(config: OnvmClientConfig) {
    this.rpcUrl = config.rpcUrl.replace(/\/$/, '');
    this.programId = config.programId;
    this.projectId = config.projectId;
    this.projectSecret = config.projectSecret;
    this.timeout = config.timeout ?? 30000;

    if (this.projectId && this.projectSecret) {
      this.derivedKeys = deriveKeys(this.projectSecret);
    }
  }

  private async detectNodeMode(): Promise<'dev' | 'prod'> {
    if (this.detectedNodeMode) {
      return this.detectedNodeMode;
    }

    try {
      const response = await fetch(`${this.rpcUrl}/health/liveness`, {
        method: 'GET',
        headers: { 'Content-Type': 'application/json' },
      });

      if (!response.ok) {
        throw new OnvmError('Failed to detect node mode', response.status);
      }

      const data: LivenessResponse = await response.json();
      this.detectedNodeMode = data.mode === 'dev' ? 'dev' : 'prod';
      
      if (this.detectedNodeMode === 'prod' && (!this.projectId || !this.projectSecret)) {
        throw new OnvmError(
          'Node is running in prod mode but projectId or projectSecret is missing. Please provide valid credentials.',
          401
        );
      }
      
      return this.detectedNodeMode;
    } catch (error) {
      if (error instanceof OnvmError) {
        throw error;
      }
      console.warn('Failed to detect node mode, defaulting to prod:', error);
      this.detectedNodeMode = 'prod';
      
      if (!this.projectId || !this.projectSecret) {
        throw new OnvmError(
          'Cannot connect to node. Assuming prod mode but projectId or projectSecret is missing.',
          401
        );
      }
      
      return 'prod';
    }
  }

  private async request<T>(
    method: string,
    path: string,
    body?: any,
    customHeaders?: Record<string, string>
  ): Promise<T> {
    const nodeMode = await this.detectNodeMode();
    
    const url = `${this.rpcUrl}${path}`;
    const bodyStr = body ? JSON.stringify(body) : '';

    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      ...customHeaders,
    };

    let authTimestamp: number | undefined;
    let authNonce: string | undefined;

    if (nodeMode === 'prod' && this.derivedKeys && this.projectId) {
      const authHeaders = buildAuthHeaders(
        method,
        path,
        bodyStr,
        this.projectId,
        this.derivedKeys.signing_key
      );
      
      authTimestamp = parseInt(authHeaders['x-timestamp']);
      authNonce = authHeaders['x-nonce'];
      
      Object.assign(headers, authHeaders);
    }

    try {
      const controller = new AbortController();
      const timeoutId = setTimeout(() => controller.abort(), this.timeout);

      const response = await fetch(url, {
        method,
        headers,
        body: bodyStr || undefined,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        const errorText = await response.text();
        
        if (response.status === 401) {
          if (nodeMode === 'prod') {
            throw new OnvmError(
              'Authentication failed. Please check your projectId and projectSecret credentials.',
              401,
              errorText
            );
          } else {
            throw new OnvmError(
              'Unauthorized access.',
              401,
              errorText
            );
          }
        }
        
        if (response.status === 403) {
          throw new OnvmError(
            'Access forbidden. Your credentials may not have permission for this operation.',
            403,
            errorText
          );
        }
        
        if (response.status === 429) {
          throw new OnvmError(
            'Rate limit exceeded. Please slow down your requests.',
            429,
            errorText
          );
        }
        
        throw new OnvmError(
          `HTTP ${response.status}: ${errorText}`,
          response.status,
          errorText
        );
      }

      const responseText = await response.text();

      if (nodeMode === 'prod' && this.derivedKeys && authTimestamp && authNonce) {
        const responseNonce = response.headers.get('x-response-nonce');
        const responseAead = response.headers.get('x-response-aead');

        if (responseAead === 'XChaCha20Poly1305' && responseNonce) {
          const { decryptResponse } = await import('./auth');
          const canonical = `${method} ${path}\n${authTimestamp}\n${authNonce}\n${bodyStr}`;

          const decrypted = decryptResponse(
            responseText,
            this.derivedKeys.response_key,
            authNonce,
            authTimestamp,
            canonical,
            responseNonce
          );
          return JSON.parse(decrypted);
        }

        if (responseText.match(/^[A-Za-z0-9+/=\s]+$/)) {
          const { decryptResponse } = await import('./auth');
          const canonical = `${method} ${path}\n${authTimestamp}\n${authNonce}\n${bodyStr}`;

          try {
            const decrypted = decryptResponse(
              responseText,
              this.derivedKeys.response_key,
              authNonce,
              authTimestamp,
              canonical,
              responseNonce || undefined
            );
            return JSON.parse(decrypted);
          } catch (decryptError) {
            throw new OnvmError(
              'Failed to decrypt response. Your credentials may be incorrect or the response format is invalid.',
              500,
              decryptError
            );
          }
        }
      }

      return responseText ? JSON.parse(responseText) : ({} as T);
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') {
        throw new OnvmNetworkError('Request timeout');
      }
      if (error instanceof OnvmError) {
        throw error;
      }
      throw new OnvmNetworkError(
        `Network error: ${error instanceof Error ? error.message : String(error)}`,
        error
      );
    }
  }

  async health(): Promise<HealthResponse> {
    return this.request<HealthResponse>('GET', '/health');
  }

  async liveness(): Promise<LivenessResponse> {
    return this.request<LivenessResponse>('GET', '/health/liveness');
  }

  async readiness(): Promise<ReadinessResponse> {
    return this.request<ReadinessResponse>('GET', '/health/readiness');
  }

  async metrics(): Promise<MetricsResponse> {
    return this.request<MetricsResponse>('GET', '/health/metrics');
  }

  async createProject(
    req: CreateProjectRequest
  ): Promise<CreateProjectResponse> {
    return this.request<CreateProjectResponse>(
      'POST',
      '/rpc-auth/projects',
      req
    );
  }

  async uploadBlob(
    data: Buffer | Uint8Array,
    mimeType?: string
  ): Promise<UploadBlobResponse> {
    const nodeMode = await this.detectNodeMode();
    const url = `${this.rpcUrl}/blobs`;
    const headers: Record<string, string> = {};

    if (mimeType) {
      headers['x-mime'] = mimeType;
    }

    let authTimestamp: number | undefined;
    let authNonce: string | undefined;
    const bodyStr = Buffer.from(data).toString('utf8');

    if (nodeMode === 'prod' && this.derivedKeys && this.projectId) {
      const authHeaders = buildAuthHeaders(
        'POST',
        '/blobs',
        bodyStr,
        this.projectId,
        this.derivedKeys.signing_key
      );
      Object.assign(headers, authHeaders);
      authTimestamp = parseInt(authHeaders['x-timestamp']);
      authNonce = authHeaders['x-nonce'];
    }

    const response = await fetch(url, {
      method: 'POST',
      headers,
      body: data,
    });

    const responseText = await response.text();

    if (!response.ok) {
      throw new OnvmError(
        `Upload failed: ${response.statusText}`,
        response.status,
        responseText
      );
    }

    if (
      nodeMode === 'prod' &&
      this.derivedKeys &&
      authTimestamp &&
      authNonce
    ) {
      const responseNonce = response.headers.get('x-response-nonce');
      const responseAead = response.headers.get('x-response-aead');

      if (responseAead === 'XChaCha20Poly1305') {
        const { decryptResponse } = await import('./auth');
        const canonical = `POST /blobs\n${authTimestamp}\n${authNonce}\n${bodyStr}`;
        const decrypted = decryptResponse(
          responseText,
          this.derivedKeys.response_key,
          authNonce,
          authTimestamp,
          canonical,
          responseNonce || undefined
        );
        return JSON.parse(decrypted);
      }
    }

    return responseText ? JSON.parse(responseText) : ({} as UploadBlobResponse);
  }

  async downloadBlob(blobId: string): Promise<Buffer> {
    const nodeMode = await this.detectNodeMode();
    const url = `${this.rpcUrl}/blobs/${blobId}`;
    const headers: Record<string, string> = {};

    let authTimestamp: number | undefined;
    let authNonce: string | undefined;

    if (nodeMode === 'prod' && this.derivedKeys && this.projectId) {
      const authHeaders = buildAuthHeaders(
        'GET',
        `/blobs/${blobId}`,
        '',
        this.projectId,
        this.derivedKeys.signing_key
      );
      Object.assign(headers, authHeaders);
      authTimestamp = parseInt(authHeaders['x-timestamp']);
      authNonce = authHeaders['x-nonce'];
    }

    const response = await fetch(url, {
      method: 'GET',
      headers,
    });

    if (!response.ok) {
      throw new OnvmError(
        `Download failed: ${response.statusText}`,
        response.status
      );
    }

    const responseAead = response.headers.get('x-response-aead');
    if (
      nodeMode === 'prod' &&
      this.derivedKeys &&
      authTimestamp &&
      authNonce &&
      responseAead === 'XChaCha20Poly1305'
    ) {
      const responseNonce = response.headers.get('x-response-nonce');
      const { decryptResponseToBuffer } = await import('./auth');
      const canonical = `GET /blobs/${blobId}\n${authTimestamp}\n${authNonce}\n`;
      const ciphertextBase64 = await response.text();
      const decrypted = decryptResponseToBuffer(
        ciphertextBase64,
        this.derivedKeys.response_key,
        authNonce,
        authTimestamp,
        canonical,
        responseNonce || undefined
      );
      return decrypted;
    }

    const arrayBuffer = await response.arrayBuffer();
    return Buffer.from(arrayBuffer);
  }

  async deployProgram(
    req: DeployProgramRequest
  ): Promise<DeployProgramResponse> {
    return this.request<DeployProgramResponse>('POST', '/programs', req);
  }

  async getProgramInfo(programId: string): Promise<ProgramInfo> {
    return this.request<ProgramInfo>('GET', `/programs/${programId}`);
  }

  async executeProgram(req: ExecuteRequest): Promise<ExecuteResponse> {
    return this.request<ExecuteResponse>('POST', '/execute', req);
  }

  async estimateFuel(req: EstimateFuelRequest): Promise<EstimateFuelResponse> {
    return this.request<EstimateFuelResponse>('POST', '/estimate-fuel', req);
  }

  async getFuelProfile(programId: string): Promise<FuelProfileResponse> {
    return this.request<FuelProfileResponse>(
      'GET',
      `/fuel-profile/${programId}`
    );
  }

  async getProgramReceipts(
    programId: string
  ): Promise<ProgramReceiptsResponse> {
    const receipts = await this.request<any[]>(
      'GET',
      `/programs/${programId}/receipts`
    );
    return { receipts };
  }

  async getProgramCommittee(
    programId: string
  ): Promise<ProgramCommitteeResponse> {
    return this.request<ProgramCommitteeResponse>(
      'GET',
      `/programs/${programId}/committee`
    );
  }

  async getProgramStateRoot(
    programId: string,
    key?: string
  ): Promise<ProgramStateRootResponse> {
    const path = key
      ? `/programs/${programId}/state-root?key=${encodeURIComponent(key)}`
      : `/programs/${programId}/state-root`;
    return this.request<ProgramStateRootResponse>('GET', path);
  }

  async listProgramCatalog(): Promise<ProgramCatalogResponse> {
    const catalog = await this.request<any[]>('GET', '/program-catalog');
    return { catalog };
  }

  async submitJob(req: SubmitJobRequest): Promise<SubmitJobResponse> {
    return this.request<SubmitJobResponse>('POST', '/jobs', req);
  }

  async getJobStatus(jobId: string): Promise<JobStatusResponse> {
    return this.request<JobStatusResponse>('GET', `/jobs/${jobId}`);
  }

  async getJobLogs(jobId: string): Promise<JobLogsResponse> {
    return this.request<JobLogsResponse>('GET', `/jobs/${jobId}/logs`);
  }

  async getJobOutput(jobId: string): Promise<JobOutputResponse> {
    return this.request<JobOutputResponse>('GET', `/jobs/${jobId}/output`);
  }

  async cancelJob(jobId: string): Promise<void> {
    await this.request<void>('POST', `/jobs/${jobId}/cancel`);
  }

  async listJobs(): Promise<JobListResponse> {
    return this.request<JobListResponse>('GET', '/jobs');
  }

  setProgramId(programId: string): void {
    this.programId = programId;
  }

  getProgramId(): string | undefined {
    return this.programId;
  }
}
