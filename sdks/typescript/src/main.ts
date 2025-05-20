import { StepflowStdioServer } from './server';

// Example input/output interfaces
interface AddInput {
  a: number;
  b: number;
}

interface AddOutput {
  result: number;
}

// Create server instance
const server = new StepflowStdioServer();

// Register a simple addition component
class MathOperations {
  @server.component()
  static add(input: AddInput): AddOutput {
    return { result: input.a + input.b };
  }
}

// Set schemas for the component
server.setComponentSchema(
  'add',
  {
    type: 'object',
    properties: {
      a: { type: 'number' },
      b: { type: 'number' }
    },
    required: ['a', 'b']
  },
  {
    type: 'object',
    properties: {
      result: { type: 'number' }
    },
    required: ['result']
  }
);

export function main(): void {
  // Start the server
  server.run();
}

if (require.main === module) {
  main();
}