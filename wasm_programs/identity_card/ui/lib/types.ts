// Types for the ONVM Identity Card client
// This file defines the types used by the ONVM client to interact with the identity card WASM program

export interface IdentityRequest {
  op: 'register' | 'getprofile' | 'updateprofile' | 'getcard' | 'listall' | 'stats';
  [key: string]: any;
}

export interface RegisterRequest {
  op: 'register';
  name: string;
  email: string;
  bio?: string;
  avatar_url?: string;
  theme?: string;
}

export interface GetProfileRequest {
  op: 'getprofile';
  id: string;
}

export interface UpdateProfileRequest {
  op: 'updateprofile';
  id: string;
  name?: string;
  email?: string;
  bio?: string;
  avatar_url?: string;
  theme?: string;
}

export interface GetCardRequest {
  op: 'getcard';
  id: string;
  format?: string;
}

export interface IdentityResponse {
  status: string;
  [key: string]: any;
}

export interface RegisterResponse {
  status: 'okregister';
  id: string;
  identity: Identity;
  card_svg: string;
}

export interface GetProfileResponse {
  status: 'okprofile';
  identity: Identity;
}

export interface UpdateProfileResponse {
  status: 'okupdate';
  id: string;
  identity: Identity;
}

export interface GetCardResponse {
  status: 'okcard';
  id: string;
  format: string;
  card_svg: string;
}

export interface ListAllResponse {
  status: 'oklist';
  identities: IdentitySummary[];
  total: number;
}

export interface StatsResponse {
  status: 'okstats';
  total_identities: number;
  total_storage_bytes: number;
  merkle_root: string;
}

export interface Identity {
  id: string;
  name: string;
  email: string;
  bio?: string;
  avatar_url?: string;
  theme: string;
  created_at: number;
  updated_at: number;
  checksum: string;
}

export interface IdentitySummary {
  id: string;
  name: string;
  email: string;
  theme: string;
  created_at: number;
}