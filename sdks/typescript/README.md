# StepFlow TypeScript SDK

A TypeScript SDK for creating StepFlow component plugins. This SDK allows you to easily create and register components that can be used in StepFlow workflows with full support for bidirectional communication and blob operations.

## Installation

```bash
npm install stepflow-sdk
```

## Usage

### Creating a simple component

```typescript
import { StepflowStdioServer } from 'stepflow-sdk';

// Create server instance
const server = new StepflowStdioServer();

// Define input/output interfaces
interface AddInput {
  a: number;
  b: number;
}

interface AddOutput {
  result: number;
}

// Register a component with schema
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

// Start the server
server.run();
```

### Using StepFlow Context for Advanced Features

The TypeScript SDK supports bidirectional communication through the `StepflowContext`:

```typescript
import { StepflowStdioServer, StepflowContext } from 'stepflow-sdk';

const server = new StepflowStdioServer();

// Component that uses blob operations
server.registerComponent(
  async (input: { data: any }, context?: StepflowContext): Promise<{ blobId: string }> => {
    if (!context) {
      throw new Error('Context is required for this component');
    }
    
    // Store data as a blob
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

// Component that retrieves blob data
server.registerComponent(
  async (input: { blobId: string }, context?: StepflowContext): Promise<{ data: any }> => {
    if (!context) {
      throw new Error('Context is required for this component');
    }
    
    // Retrieve data from blob
    const data = await context.getBlob(input.blobId);
    return { data };
  },
  'get_data',
  {
    description: 'Retrieves data from a blob by ID',
    inputSchema: {
      type: 'object',
      properties: {
        blobId: { type: 'string', description: 'ID of the blob to retrieve' }
      },
      required: ['blobId']
    },
    outputSchema: {
      type: 'object',
      properties: {
        data: { description: 'Retrieved blob data' }
      },
      required: ['data']
    }
  }
);

server.run();
```

### StepflowContext API

The `StepflowContext` provides the following methods:

- `putBlob(data: any): Promise<string>` - Store data as a blob and return its ID
- `getBlob(blobId: string): Promise<any>` - Retrieve data by blob ID
- `evaluateFlow(flow: any, input: any): Promise<any>` - Evaluate a nested flow

### Protocol Support

This SDK supports the latest StepFlow protocol including:

- **Updated method names**: `components/list`, `components/info`, `components/execute`
- **Bidirectional communication**: Components can make requests back to the StepFlow runtime
- **Blob operations**: Store and retrieve data using content-based addressing
- **Flow evaluation**: Execute nested workflows from within components
- **Rich component metadata**: Include descriptions and detailed schemas

## Development

### Building

```bash
npm run build
```

### Testing

```bash
npm test
```

### Linting

```bash
npm run lint
```

## License

MIT