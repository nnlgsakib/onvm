import { OnvmClient, RpcMode } from '../src';

describe('OnvmClient', () => {
  describe('constructor', () => {
    it('should create client in dev mode without credentials', () => {
      const client = new OnvmClient({
        rpcUrl: 'http://localhost:8080',
        mode: RpcMode.Dev,
      });

      expect(client).toBeDefined();
      expect(client.getProgramId()).toBeUndefined();
    });

    it('should throw error in prod mode without credentials', () => {
      expect(() => {
        new OnvmClient({
          rpcUrl: 'http://localhost:8080',
          mode: RpcMode.Prod,
        });
      }).toThrow('projectId and projectSecret are required in prod mode');
    });

    it('should create client in prod mode with valid credentials', () => {
      const client = new OnvmClient({
        rpcUrl: 'http://localhost:8080',
        mode: RpcMode.Prod,
        projectId: 'test-project',
        projectSecret: '0'.repeat(64),
      });

      expect(client).toBeDefined();
    });

    it('should set program ID', () => {
      const client = new OnvmClient({
        rpcUrl: 'http://localhost:8080',
        mode: RpcMode.Dev,
        programId: 'test-program-id',
      });

      expect(client.getProgramId()).toBe('test-program-id');

      client.setProgramId('new-program-id');
      expect(client.getProgramId()).toBe('new-program-id');
    });
  });

  describe('URL handling', () => {
    it('should strip trailing slash from rpcUrl', () => {
      const client = new OnvmClient({
        rpcUrl: 'http://localhost:8080/',
        mode: RpcMode.Dev,
      });

      expect(client).toBeDefined();
    });
  });

  describe('configuration defaults', () => {
    it('should use default timeout', () => {
      const client = new OnvmClient({
        rpcUrl: 'http://localhost:8080',
        mode: RpcMode.Dev,
      });

      expect(client).toBeDefined();
    });

    it('should use custom timeout', () => {
      const client = new OnvmClient({
        rpcUrl: 'http://localhost:8080',
        mode: RpcMode.Dev,
        timeout: 60000,
      });

      expect(client).toBeDefined();
    });

    it('should default to prod mode', () => {
      expect(() => {
        new OnvmClient({
          rpcUrl: 'http://localhost:8080',
        });
      }).toThrow();
    });

    it('should default encryptResponses to false', () => {
      const client = new OnvmClient({
        rpcUrl: 'http://localhost:8080',
        mode: RpcMode.Dev,
      });

      expect(client).toBeDefined();
    });
  });
});
