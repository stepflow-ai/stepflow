import { protocolSchema, methodRequest, methodResponseSuccess } from '../src/protocolSchema';

describe('Protocol Schema', () => {
  test('validates a method request', () => {
    const request = {
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {
        clientInfo: {
          name: 'test-client',
          version: '1.0.0'
        }
      }
    };

    const result = methodRequest.safeParse(request);
    expect(result.success).toBe(true);
  });

  test('validates a method response', () => {
    const response = {
      jsonrpc: '2.0',
      id: 1,
      result: {
        serverInfo: {
          name: 'test-server',
          version: '1.0.0'
        }
      }
    };

    const result = methodResponseSuccess.safeParse(response);
    expect(result.success).toBe(true);
  });

  test('validates a component execute request', () => {
    const request = {
      jsonrpc: '2.0',
      id: '123',
      method: 'componentExecute',
      params: {
        component: 'http://example.com/component1',
        args: {
          param1: 'value1',
          param2: 42
        }
      }
    };

    const result = protocolSchema.safeParse(request);
    expect(result.success).toBe(true);
  });

  test('validates a notification', () => {
    const notification = {
      jsonrpc: '2.0',
      method: 'initialized',
      params: {}
    };

    const result = protocolSchema.safeParse(notification);
    expect(result.success).toBe(true);
  });
});
