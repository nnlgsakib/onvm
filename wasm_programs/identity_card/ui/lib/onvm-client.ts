// ONVM Client for Identity Card WASM Program
// This client interacts with the ONVM RPC to register and manage digital identities

import type {
  IdentityRequest,
  RegisterRequest,
  GetProfileRequest,
  UpdateProfileRequest,
  GetCardRequest,
  RegisterResponse,
  GetProfileResponse,
  UpdateProfileResponse,
  GetCardResponse,
  ListAllResponse,
  StatsResponse
} from '@/lib/types'

const RPC_ENDPOINT = 'http://localhost:8080';

export class ONVMClient {
  private rpcEndpoint: string;

  constructor(rpcEndpoint: string = RPC_ENDPOINT) {
    this.rpcEndpoint = rpcEndpoint;
  }

  // Register a new identity
  async registerIdentity(request: RegisterRequest): Promise<RegisterResponse> {
    return this.executeProgram(request);
  }

  // Get an identity profile
  async getIdentity(request: GetProfileRequest): Promise<GetProfileResponse> {
    return this.executeProgram(request);
  }

  // Update an identity profile
  async updateIdentity(request: UpdateProfileRequest): Promise<UpdateProfileResponse> {
    return this.executeProgram(request);
  }

  // Get an identity card SVG
  async getIdentityCard(request: GetCardRequest): Promise<GetCardResponse> {
    return this.executeProgram(request);
  }

  // List all identities
  async listAllIdentities(): Promise<ListAllResponse> {
    return this.executeProgram({ op: 'listall' });
  }

  // Get statistics
  async getStats(): Promise<StatsResponse> {
    return this.executeProgram({ op: 'stats' });
  }

  // Generic execute program method
  private async executeProgram<T extends { status: string }>(
    request: IdentityRequest
  ): Promise<T> {
    try {
      // Get program ID from environment or fallback
      const programId = "918c95aa44b129ae2315698637afbb339b7cb43ed6115e433605ae172f617bc4";
      if (!programId) {
        throw new Error('NEXT_PUBLIC_IDENTITY_CARD_PROGRAM_ID is not set');
      }

      // Encode request as base64
      const inputBase64 = Buffer.from(JSON.stringify(request)).toString('base64');

      // Execute program via ONVM RPC
      const response = await fetch(`${this.rpcEndpoint}/execute`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          program_id: programId,
          input_base64: inputBase64,
        }),
      });

      if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`RPC error: ${response.status} - ${errorText}`);
      }

      const result = await response.json();
      
      // Decode the response
      const returnData = Buffer.from(result.return_base64, 'base64').toString('utf-8');
      const parsedResponse = JSON.parse(returnData);
      
      if (parsedResponse.status === 'err') {
        throw new Error(parsedResponse.message || 'Unknown error occurred');
      }
      
      return parsedResponse as T;
    } catch (error) {
      console.error('ONVM Client Error:', error);
      throw error;
    }
  }

  // Helper method to upload a WASM program
  async uploadWasm(wasmBuffer: ArrayBuffer): Promise<string> {
    try {
      // Upload WASM blob
      const blobResponse = await fetch(`${this.rpcEndpoint}/blobs`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/wasm',
        },
        body: wasmBuffer,
      });

      if (!blobResponse.ok) {
        const errorText = await blobResponse.text();
        throw new Error(`Blob upload error: ${blobResponse.status} - ${errorText}`);
      }

      const blobResult = await blobResponse.json();
      const blobId = blobResult.id;

      // Deploy program
      const deployResponse = await fetch(`${this.rpcEndpoint}/programs`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          wasm_base64: Buffer.from(wasmBuffer).toString('base64'),
          entrypoint: 'onvm_main',
          blob_refs: [],
        }),
      });

      if (!deployResponse.ok) {
        const errorText = await deployResponse.text();
        throw new Error(`Program deploy error: ${deployResponse.status} - ${errorText}`);
      }

      const deployResult = await deployResponse.json();
      return deployResult.id;
    } catch (error) {
      console.error('WASM Upload Error:', error);
      throw error;
    }
  }
}

// Create a singleton instance
export const onvmClient = new ONVMClient();

export default onvmClient;