import { OnvmClient } from 'onvm-sdk';
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

const RPC_ENDPOINT = process.env.NEXT_PUBLIC_RPC_URL || 'http://localhost:8080';
const PROGRAM_ID = process.env.NEXT_PUBLIC_IDENTITY_CARD_PROGRAM_ID || "progfdad95913d574dd1d22c9950f37a967720b65911666224ab64119c786a4be023";
const PROJECT_ID = process.env.NEXT_PUBLIC_PROJECT_ID || "b72724722db7b33071c5e7cf5d10bc53";
const PROJECT_SECRET = process.env.NEXT_PUBLIC_PROJECT_SECRET || "65eb796b7044c49273d44475608b5f1aa892de2f9b2d6a10c59ac954739bacdc";

export class ONVMClient {
  private client: OnvmClient;
  private programId: string;

  constructor() {
    this.client = new OnvmClient({
      rpcUrl: RPC_ENDPOINT,
      programId: PROGRAM_ID,
      projectId: PROJECT_ID,
      projectSecret: PROJECT_SECRET,
      timeout: 30000,
    });

    this.programId = PROGRAM_ID;
  }

  async registerIdentity(request: RegisterRequest): Promise<RegisterResponse> {
    return this.executeProgram(request);
  }

  async getIdentity(request: GetProfileRequest): Promise<GetProfileResponse> {
    return this.executeProgram(request);
  }

  async updateIdentity(request: UpdateProfileRequest): Promise<UpdateProfileResponse> {
    return this.executeProgram(request);
  }

  async getIdentityCard(request: GetCardRequest): Promise<GetCardResponse> {
    return this.executeProgram(request);
  }

  async listAllIdentities(): Promise<ListAllResponse> {
    return this.executeProgram({ op: 'listall' });
  }

  async getStats(): Promise<StatsResponse> {
    return this.executeProgram({ op: 'stats' });
  }

  private async executeProgram<T extends { status: string }>(
    request: IdentityRequest
  ): Promise<T> {
    const inputBase64 = Buffer.from(JSON.stringify(request)).toString('base64');

    const result = await this.client.executeProgram({
      program_id: this.programId,
      input_base64: inputBase64,
    });

    const returnData = Buffer.from(result.return_base64, 'base64').toString('utf-8');
    const parsedResponse = JSON.parse(returnData);

    if (parsedResponse.status === 'err') {
      throw new Error(parsedResponse.message || 'Unknown error occurred');
    }

    return parsedResponse as T;
  }
}

export const onvmClient = new ONVMClient();

export default onvmClient;
