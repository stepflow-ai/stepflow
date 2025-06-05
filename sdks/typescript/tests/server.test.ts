import { StepflowStdioServer } from '../src/server';
import { Incoming } from '../src/transport';
import { v4 as uuidv4 } from 'uuid';

// Mock stdout and stderr for testing
const originalStdoutWrite = process.stdout.write;
const originalStderrWrite = process.stderr.write;
let stdoutOutput: string[] = [];
let stderrOutput: string[] = [];

beforeEach(() => {
  // Clear outputs
  stdoutOutput = [];
  stderrOutput = [];

  // Mock stdout and stderr
  process.stdout.write = jest.fn((data: string | Uint8Array) => {
    stdoutOutput.push(data.toString());
    return true;
  });

  process.stderr.write = jest.fn((data: string | Uint8Array) => {
    stderrOutput.push(data.toString());
    return true;
  });
});

afterEach(() => {
  // Restore original implementations
  process.stdout.write = originalStdoutWrite;
  process.stderr.write = originalStderrWrite;
});

// Test message classes
interface ValidInput {
  name: string;
  age: number;
}

interface ValidOutput {
  greeting: string;
  age_next_year: number;
}

describe('StepflowStdioServer', () => {
  let server: StepflowStdioServer;

  beforeEach(() => {
    server = new StepflowStdioServer();
  });

  test('component registration with decorator', () => {
    // Manual registration instead of using decorator for test
    const testFunc = (input: ValidInput): ValidOutput => {
      return {
        greeting: `Hello ${input.name}!`,
        age_next_year: input.age + 1
      };
    };

    // Manually register the component
    server.registerComponent(testFunc, 'testComponent');

    // Set schemas
    server.setComponentSchema(
      'testComponent',
      {
        type: 'object',
        properties: {
          name: { type: 'string' },
          age: { type: 'number' }
        },
        required: ['name', 'age']
      },
      {
        type: 'object',
        properties: {
          greeting: { type: 'string' },
          age_next_year: { type: 'number' }
        },
        required: ['greeting', 'age_next_year']
      }
    );

    const component = server.getComponent('testComponent');
    expect(component).toBeDefined();
    expect(component?.name).toBe('testComponent');
    expect(typeof component?.function).toBe('function');
  });

  test('component with custom name', () => {
    // Manual registration instead of using decorator for test
    const testFunc = (input: ValidInput): ValidOutput => {
      return {
        greeting: `Hello ${input.name}!`,
        age_next_year: input.age + 1
      };
    };

    // Manually register the component
    server.registerComponent(testFunc, 'custom_name');

    const component = server.getComponent('custom_name');
    expect(component).toBeDefined();
    expect(component?.name).toBe('custom_name');
    expect(typeof component?.function).toBe('function');
  });

  test('component execution', () => {
    // Manual registration instead of using decorator for test
    const testFunc = (input: ValidInput): ValidOutput => {
      return {
        greeting: `Hello ${input.name}!`,
        age_next_year: input.age + 1
      };
    };

    // Manually register the component
    server.registerComponent(testFunc, 'testComponent');

    const component = server.getComponent('testComponent');
    expect(component).toBeDefined();

    if (component) {
      const result = component.function({ name: 'Alice', age: 25 }) as ValidOutput;
      expect(result).toBeDefined();
      expect(result.greeting).toBe('Hello Alice!');
      expect(result.age_next_year).toBe(26);
    }
  });

  test('list components', () => {
    // Manual registration instead of using decorator for test
    const testFunc1 = (input: ValidInput): ValidOutput => {
      return { greeting: '', age_next_year: 0 };
    };

    const testFunc2 = (input: ValidInput): ValidOutput => {
      return { greeting: '', age_next_year: 0 };
    };

    // Manually register components
    server.registerComponent(testFunc1, 'component1');
    server.registerComponent(testFunc2, 'component2');

    // Mark server as initialized for testing
    (server as any).initialized = true;

    const request: Incoming = {
      jsonrpc: '2.0',
      id: uuidv4(),
      method: 'list_components',
      params: {}
    };

    return server['handleMessage'](request).then(response => {
      expect(response).toBeDefined();
      if (response) {
        expect(response.id).toBe(request.id);
        expect(response.result).toBeDefined();
        if (response.result) {
          expect(response.result.components).toBeInstanceOf(Array);
          expect(response.result.components.length).toBe(2);
          expect(response.result.components).toContain('component1');
          expect(response.result.components).toContain('component2');
        }
      }
    });
  });

  test('handle initialize', () => {
    const request: Incoming = {
      jsonrpc: '2.0',
      id: uuidv4(),
      method: 'initialize',
      params: {}
    };

    return server['handleMessage'](request).then(response => {
      expect(response).toBeDefined();
      if (response) {
        expect(response.id).toBe(request.id);
        expect(response.result).toBeDefined();
        if (response.result) {
          expect(response.result.server_protocol_version).toBe(1);
        }
      }

      expect(server['initialized']).toBe(false);

      const notification: Incoming = {
        jsonrpc: '2.0',
        method: 'initialized',
        params: {}
      };

      return server['handleMessage'](notification).then(notificationResponse => {
        expect(notificationResponse).toBeNull();
        expect(server['initialized']).toBe(true);
      });
    });
  });

  test('handle component_info', () => {
    // Manual registration instead of using decorator for test
    const testFunc = (input: ValidInput): ValidOutput => {
      return { greeting: '', age_next_year: 0 };
    };

    // Manually register the component
    server.registerComponent(testFunc, 'test_component');

    // Set schemas
    server.setComponentSchema(
      'test_component',
      { type: 'object', properties: { name: { type: 'string' }, age: { type: 'number' } } },
      { type: 'object', properties: { greeting: { type: 'string' }, age_next_year: { type: 'number' } } }
    );

    // Mark server as initialized for testing
    (server as any).initialized = true;

    const request: Incoming = {
      jsonrpc: '2.0',
      id: uuidv4(),
      method: 'component_info',
      params: { component: 'test_component' }
    };

    return server['handleMessage'](request).then(response => {
      expect(response).toBeDefined();
      if (response) {
        expect(response.id).toBe(request.id);
        expect(response.result).toBeDefined();
        if (response.result) {
          expect(response.result.input_schema).toBeDefined();
          expect(response.result.output_schema).toBeDefined();
        }
      }
    });
  });

  test('handle component_info not found', () => {
    // Mark server as initialized for testing
    (server as any).initialized = true;

    const request: Incoming = {
      jsonrpc: '2.0',
      id: uuidv4(),
      method: 'component_info',
      params: { component: 'non_existent' }
    };

    return server['handleMessage'](request).then(response => {
      expect(response).toBeDefined();
      if (response) {
        expect(response.id).toBe(request.id);
        expect(response.error).toBeDefined();
        if (response.error) {
          expect(response.error.code).toBe(-32601);
          expect(response.error.message).toContain('not found');
        }
      }
    });
  });

  test('handle component_execute', () => {
    // Mark server as initialized for testing
    (server as any).initialized = true;

    // Manual registration instead of using decorator for test
    const testFunc = (input: ValidInput): ValidOutput => {
      return {
        greeting: `Hello ${input.name}!`,
        age_next_year: input.age + 1
      };
    };

    // Manually register the component
    server.registerComponent(testFunc, 'test_component');

    const request: Incoming = {
      jsonrpc: '2.0',
      id: uuidv4(),
      method: 'component_execute',
      params: {
        component: 'test_component',
        input: {
          name: 'Alice',
          age: 25
        }
      }
    };

    return server['handleMessage'](request).then(response => {
      expect(response).toBeDefined();
      if (response) {
        expect(response.id).toBe(request.id);
        expect(response.result).toBeDefined();
        if (response.result) {
          expect(response.result.output).toBeDefined();
          expect(response.result.output.greeting).toBe('Hello Alice!');
          expect(response.result.output.age_next_year).toBe(26);
        }
      }
    });
  });

  test('handle component_execute invalid input', () => {
    // Mark server as initialized for testing
    (server as any).initialized = true;

    // Manual registration instead of using decorator for test
    const testFunc = (input: ValidInput): ValidOutput => {
      // This will throw if input is invalid
      if (!input.name || typeof input.age !== 'number') {
        throw new Error('Invalid input');
      }
      return {
        greeting: `Hello ${input.name}!`,
        age_next_year: input.age + 1
      };
    };

    // Manually register the component
    server.registerComponent(testFunc, 'test_component');

    const request: Incoming = {
      jsonrpc: '2.0',
      id: uuidv4(),
      method: 'component_execute',
      params: {
        component: 'test_component',
        input: {
          invalid: 'input'
        }
      }
    };

    return server['handleMessage'](request).then(response => {
      expect(response).toBeDefined();
      if (response) {
        expect(response.id).toBe(request.id);
        expect(response.error).toBeDefined();
        if (response.error) {
          expect(response.error.code).toBe(-32000);
        }
      }
    });
  });

  test('handle unknown method', () => {
    // Mark server as initialized for testing
    (server as any).initialized = true;

    const request: Incoming = {
      jsonrpc: '2.0',
      id: uuidv4(),
      method: 'unknown_method',
      params: {}
    };

    return server['handleMessage'](request).then(response => {
      expect(response).toBeDefined();
      if (response) {
        expect(response.id).toBe(request.id);
        expect(response.error).toBeDefined();
        if (response.error) {
          expect(response.error.code).toBe(-32601);
          expect(response.error.message).toContain('Unknown method');
        }
      }
    });
  });

  test('uninitialized server', () => {
    const request: Incoming = {
      jsonrpc: '2.0',
      id: uuidv4(),
      method: 'list_components',
      params: {}
    };

    return server['handleMessage'](request).then(response => {
      expect(response).toBeDefined();
      if (response) {
        expect(response.id).toBe(request.id);
        expect(response.error).toBeDefined();
        if (response.error) {
          expect(response.error.code).toBe(-32002);
          expect(response.error.message).toContain('not initialized');
        }
      }
    });
  });
});