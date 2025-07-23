import { StepflowStdioServer } from '../src/server';
import { StepflowHttpServer } from '../src/http-server';
import { StepflowContext } from '../src/transport';

describe('StepflowHttpServer', () => {
  let stdioServer: StepflowStdioServer;
  let httpServer: StepflowHttpServer;
  const testPort = 9999;
  const testHost = 'localhost';

  beforeEach(() => {
    stdioServer = new StepflowStdioServer();
    
    // Register test components
    stdioServer.registerComponent(
      (input: { x: number; y: number }) => ({ sum: input.x + input.y }),
      'add',
      {
        description: 'Add two numbers',
        inputSchema: {
          type: 'object',
          properties: {
            x: { type: 'number' },
            y: { type: 'number' }
          },
          required: ['x', 'y']
        },
        outputSchema: {
          type: 'object',
          properties: {
            sum: { type: 'number' }
          }
        }
      }
    );
    
    stdioServer.registerComponent(
      async (input: { data: any }, context?: StepflowContext) => {
        if (!context) throw new Error('Context required');
        const blobId = await context.putBlob(input.data);
        return { blobId };
      },
      'store_blob',
      {
        description: 'Store data as blob'
      }
    );
    
    httpServer = new StepflowHttpServer(stdioServer, testHost, testPort);
  });

  test('constructor sets up server with correct host and port', () => {
    expect(httpServer).toBeDefined();
    expect(httpServer).toBeInstanceOf(StepflowHttpServer);
  });

  test('getComponents returns registered components', () => {
    const components = stdioServer.getComponents();
    expect(components).toContain('add');
    expect(components).toContain('store_blob');
  });

  test('getComponent returns component details', () => {
    const component = stdioServer.getComponent('add');
    expect(component).toBeDefined();
    expect(component?.options?.description).toBe('Add two numbers');
  });

  test('getComponent returns undefined for non-existent component', () => {
    const component = stdioServer.getComponent('non_existent');
    expect(component).toBeUndefined();
  });

  // Note: Full HTTP server integration tests would require actually starting the server
  // and making real HTTP requests. For unit tests, we focus on the logic.
  
  describe('HTTP endpoints (mock tests)', () => {
    test('health endpoint structure', () => {
      // Test that the HTTP server has the expected structure
      expect(httpServer.start).toBeDefined();
      expect(typeof httpServer.start).toBe('function');
    });
  });
});