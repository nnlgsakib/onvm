import { deriveKeys, generateNonce, signRequest } from '../src/auth';

describe('Auth Module', () => {
  const testSecret = '0'.repeat(64);

  describe('deriveKeys', () => {
    it('should derive signing and response keys from project secret', () => {
      const keys = deriveKeys(testSecret);

      expect(keys.signing_key).toBeDefined();
      expect(keys.response_key).toBeDefined();
      expect(keys.signing_key.length).toBe(32);
      expect(keys.response_key.length).toBe(32);
    });

    it('should throw error for invalid secret length', () => {
      expect(() => deriveKeys('invalid')).toThrow(
        'Project secret must be 32 bytes'
      );
    });

    it('should produce deterministic keys', () => {
      const keys1 = deriveKeys(testSecret);
      const keys2 = deriveKeys(testSecret);

      expect(keys1.signing_key.equals(keys2.signing_key)).toBe(true);
      expect(keys1.response_key.equals(keys2.response_key)).toBe(true);
    });
  });

  describe('generateNonce', () => {
    it('should generate 32-character hex nonce', () => {
      const nonce = generateNonce();

      expect(nonce).toHaveLength(32);
      expect(/^[0-9a-f]+$/.test(nonce)).toBe(true);
    });

    it('should generate unique nonces', () => {
      const nonce1 = generateNonce();
      const nonce2 = generateNonce();

      expect(nonce1).not.toBe(nonce2);
    });
  });

  describe('signRequest', () => {
    it('should sign request with correct canonical format', () => {
      const keys = deriveKeys(testSecret);
      const signature = signRequest(
        'POST',
        '/execute',
        '{"program_id":"test"}',
        1234567890,
        'test-nonce',
        keys.signing_key
      );

      expect(signature).toBeDefined();
      expect(signature.length).toBe(64);
      expect(/^[0-9a-f]+$/.test(signature)).toBe(true);
    });

    it('should produce deterministic signatures', () => {
      const keys = deriveKeys(testSecret);
      const sig1 = signRequest(
        'POST',
        '/execute',
        '{"test":true}',
        1000,
        'nonce1',
        keys.signing_key
      );
      const sig2 = signRequest(
        'POST',
        '/execute',
        '{"test":true}',
        1000,
        'nonce1',
        keys.signing_key
      );

      expect(sig1).toBe(sig2);
    });

    it('should produce different signatures for different inputs', () => {
      const keys = deriveKeys(testSecret);
      const sig1 = signRequest(
        'POST',
        '/execute',
        '{"test":true}',
        1000,
        'nonce1',
        keys.signing_key
      );
      const sig2 = signRequest(
        'POST',
        '/execute',
        '{"test":false}',
        1000,
        'nonce1',
        keys.signing_key
      );

      expect(sig1).not.toBe(sig2);
    });
  });
});
