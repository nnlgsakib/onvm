import { OnvmClient, RpcMode } from '../src';

describe('ONVM SDK', () => {
  it('exports OnvmClient', () => {
    expect(OnvmClient).toBeDefined();
  });

  it('exports RpcMode', () => {
    expect(RpcMode).toBeDefined();
    expect(RpcMode.Dev).toBe('dev');
    expect(RpcMode.Prod).toBe('prod');
  });
});