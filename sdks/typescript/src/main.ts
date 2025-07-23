import { StepflowStdioServer } from './server';
import { StepflowContext } from './transport';
import { udf, udfSchema } from './udf';

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
server.registerComponent(
  (input: AddInput): AddOutput => {
    return { result: input.a + input.b };
  },
  'add',
  {
    description: 'Adds two numbers together',
    inputSchema: {
      type: 'object',
      properties: {
        a: { type: 'number', description: 'First number' },
        b: { type: 'number', description: 'Second number' }
      },
      required: ['a', 'b']
    },
    outputSchema: {
      type: 'object',
      properties: {
        result: { type: 'number', description: 'Sum of a and b' }
      },
      required: ['result']
    }
  }
);

// Example component that uses the context for blob operations
server.registerComponent(
  async (input: { data: any }, context?: StepflowContext): Promise<{ blobId: string }> => {
    if (!context) {
      throw new Error('Context is required for this component');
    }
    
    // Store the data as a blob
    const blobId = await context.putBlob(input.data);
    return { blobId };
  },
  'store_data',
  {
    description: 'Stores data as a blob and returns the blob ID',
    inputSchema: {
      type: 'object',
      properties: {
        data: { description: 'Data to store as blob' }
      },
      required: ['data']
    },
    outputSchema: {
      type: 'object',
      properties: {
        blobId: { type: 'string', description: 'ID of the stored blob' }
      },
      required: ['blobId']
    }
  }
);

// Register the UDF component
server.registerComponent(
  udf,
  'udf',
  udfSchema.component
);

export function main(): void {
  // Start the server
  server.run();
}

if (require.main === module) {
  main();
}