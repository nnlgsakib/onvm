import { createHmac, randomBytes } from 'crypto';
import { OnvmAuthError } from './errors';
import { XChaCha20Poly1305 } from '@stablelib/xchacha20poly1305';

const HDR_PROJECT_ID = 'x-project-id';
const HDR_TIMESTAMP = 'x-timestamp';
const HDR_NONCE = 'x-nonce';
const HDR_SIGNATURE = 'x-signature';

export interface DerivedKeys {
  signing_key: Buffer;
  response_key: Buffer;
}

/**
 * HKDF implementation using HMAC-SHA256
 * Matches the Rust implementation in src/rpc/auth.rs
 */
function hkdf(
  ikm: Buffer,
  salt: Buffer | undefined,
  info: Buffer,
  length: number
): Buffer {
  // Extract phase
  const actualSalt = salt || Buffer.alloc(32, 0);
  const prk = createHmac('sha256', actualSalt).update(ikm).digest();
  
  // Expand phase
  const n = Math.ceil(length / 32);
  const blocks: Buffer[] = [];
  let t = Buffer.alloc(0);
  
  for (let i = 1; i <= n; i++) {
    const hmac = createHmac('sha256', prk);
    hmac.update(t);
    hmac.update(info);
    hmac.update(Buffer.from([i]));
    t = hmac.digest();
    blocks.push(t);
  }
  
  return Buffer.concat(blocks).slice(0, length);
}

/**
 * Derive signing and response keys from project secret
 * Matches derive_keys() in src/rpc/auth.rs
 */
export function deriveKeys(projectSecret: string): DerivedKeys {
  const secretBuffer = Buffer.from(projectSecret, 'hex');
  if (secretBuffer.length !== 32) {
    throw new OnvmAuthError('Project secret must be 32 bytes (64 hex chars)');
  }

  // HKDF with no salt, info="onvm-rpc-auth", output=64 bytes
  const okm = hkdf(
    secretBuffer,
    undefined,
    Buffer.from('onvm-rpc-auth'),
    64
  );

  return {
    signing_key: okm.slice(0, 32),
    response_key: okm.slice(32, 64),
  };
}

/**
 * Generate a cryptographically secure random nonce
 */
export function generateNonce(): string {
  return randomBytes(16).toString('hex');
}

/**
 * Get current timestamp in milliseconds
 */
export function getCurrentTimestampMs(): number {
  return Date.now();
}

/**
 * Sign a request using HMAC-SHA256
 * Matches the canonical format in src/rpc/auth.rs
 */
export function signRequest(
  method: string,
  path: string,
  body: string,
  timestamp: number,
  nonce: string,
  signingKey: Buffer
): string {
  // Canonical format: "METHOD PATH\nTIMESTAMP\nNONCE\nBODY"
  const canonical = `${method} ${path}\n${timestamp}\n${nonce}\n${body}`;
  const hmac = createHmac('sha256', signingKey);
  hmac.update(canonical);
  return hmac.digest('hex');
}

/**
 * Build authentication headers for a request
 * Returns headers: x-project-id, x-timestamp, x-nonce, x-signature
 */
export function buildAuthHeaders(
  method: string,
  path: string,
  body: string,
  projectId: string,
  signingKey: Buffer
): Record<string, string> {
  const timestamp = getCurrentTimestampMs();
  const nonce = generateNonce();
  const signature = signRequest(method, path, body, timestamp, nonce, signingKey);

  return {
    [HDR_PROJECT_ID]: projectId,
    [HDR_TIMESTAMP]: timestamp.toString(),
    [HDR_NONCE]: nonce,
    [HDR_SIGNATURE]: signature,
  };
}

/**
 * Derive response nonce from response key and request nonce
 * Matches derive_response_nonce() in src/rpc/auth.rs
 */
function deriveResponseNonce(
  responseKey: Buffer,
  requestNonce: string,
  timestamp: number
): Buffer {
  const info = Buffer.from(`resp-${timestamp}`);
  
  // HKDF with salt=responseKey, ikm=requestNonce, info="resp-{timestamp}", output=24 bytes
  const nonce = hkdf(
    Buffer.from(requestNonce),
    responseKey,
    info,
    24
  );
  
  return nonce;
}

/**
 * Decrypt an encrypted response using XChaCha20-Poly1305
 * Matches the encryption in src/rpc/auth.rs
 */
export function decryptResponse(
  ciphertextBase64: string,
  responseKey: Buffer,
  requestNonce: string,
  timestamp: number,
  canonical: string,
  responseNonceHex?: string
): string {
  const plaintext = decryptResponseToBuffer(
    ciphertextBase64,
    responseKey,
    requestNonce,
    timestamp,
    canonical,
    responseNonceHex
  );
  return plaintext.toString("utf8");
}

/**
 * Decrypt an encrypted response and return the raw bytes.
 * Useful for binary endpoints like blob downloads.
 */
export function decryptResponseToBuffer(
  ciphertextBase64: string,
  responseKey: Buffer,
  requestNonce: string,
  timestamp: number,
  canonical: string,
  responseNonceHex?: string
): Buffer {
  const ciphertext = Buffer.from(ciphertextBase64, 'base64');

  // Use provided nonce or derive it
  const nonce = responseNonceHex
    ? Buffer.from(responseNonceHex, 'hex')
    : deriveResponseNonce(responseKey, requestNonce, timestamp);

  if (nonce.length !== 24) {
    throw new OnvmAuthError('Invalid response nonce length: expected 24 bytes');
  }

  try {
    // XChaCha20-Poly1305 AEAD with canonical string as AAD
    const aead = new XChaCha20Poly1305(new Uint8Array(responseKey));
    const plaintext = aead.open(
      new Uint8Array(nonce),
      new Uint8Array(ciphertext),
      new Uint8Array(Buffer.from(canonical))
    );
    
    if (!plaintext) {
      throw new Error('Decryption failed: authentication tag mismatch');
    }
    
    return Buffer.from(plaintext);
  } catch (error) {
    throw new OnvmAuthError(`Response decryption failed: ${error}`);
  }
}
