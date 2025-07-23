# StepFlow TypeScript SDK

A TypeScript SDK for creating StepFlow component plugins. This SDK allows you to create and register components that can be used in StepFlow workflows with full support for bidirectional communication, blob operations, user-defined functions (UDF), and both stdio and HTTP server modes.

## Features

- 🚀 **Stdio and HTTP Server Modes** - Run components via stdin/stdout or as an HTTP server
- 🔧 **User-Defined Functions (UDF)** - Execute dynamic JavaScript code stored as blobs
- 💾 **Blob Operations** - Store and retrieve data using content-based addressing
- 🔄 **Bidirectional Communication** - Components can make requests back to the StepFlow runtime
- 🏗️ **Type-Safe Component Development** - Full TypeScript support with type inference
- 🔌 **Session Isolation** (HTTP mode) - Each client gets its own isolated session context
- 📡 **Server-Sent Events** (HTTP mode) - Real-time bidirectional communication via SSE

## Installation

```bash
npm install stepflow-sdk
```

## Quick Start

### Command Line Interface

The SDK can be run in two modes:

**Stdio Mode (default):**
```bash
# Run with ts-node during development
npx ts-node src/main.ts

# Or after building
node dist/main.js
```

**HTTP Mode:**
```bash
# Run HTTP server on default port 8080
npx ts-node src/main.ts --http

# Run on custom host and port
npx ts-node src/main.ts --http --host 0.0.0.0 --port 3000
```

### Creating Components

#### Simple Component

```typescript
import { StepflowStdioServer } from 'stepflow-sdk';

const server = new StepflowStdioServer();

// Define input/output interfaces for type safety
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

server.run();
```

#### Component with Context (Blob Operations)

```typescript
import { StepflowStdioServer, StepflowContext } from 'stepflow-sdk';

const server = new StepflowStdioServer();

// Component that stores data as a blob
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
    description: 'Stores data as a blob and returns the blob ID'
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
    description: 'Retrieves data from a blob by ID'
  }
);

server.run();
```

### User-Defined Functions (UDF)

The SDK includes a built-in UDF component that can execute JavaScript code stored as blobs:

```typescript
import { StepflowStdioServer, udf, udfSchema } from 'stepflow-sdk';

const server = new StepflowStdioServer();

// Register the UDF component
server.registerComponent(udf, 'udf', udfSchema.component);

server.run();
```

**UDF Blob Format:**
```json
{
  "code": "input.x + input.y",
  "input_schema": {
    "type": "object",
    "properties": {
      "x": { "type": "number" },
      "y": { "type": "number" }
    }
  }
}
```

**Supported Code Patterns:**

1. **Simple Expression:**
```javascript
"input.x + input.y"
```

2. **Multi-line Code:**
```javascript
`const sum = input.values.reduce((a, b) => a + b, 0);
const avg = sum / input.values.length;
return { sum, avg };`
```

3. **Named Function:**
```javascript
{
  "function_name": "processData",
  "code": `
    function processData(input) {
      return {
        doubled: input.value * 2,
        squared: input.value * input.value
      };
    }
  `
}
```

### HTTP Server Mode

The HTTP server mode enables remote component execution with session isolation:

```typescript
import { runCLI } from 'stepflow-sdk';

// Run with CLI arguments
process.argv = ['node', 'server.js', '--http', '--port', '8080'];
runCLI();
```

**Connecting to HTTP Server:**

1. **Get Session ID via SSE:**
```bash
curl -N http://localhost:8080/runtime/events
# Returns: event: endpoint
#          data: {"endpoint":"/?sessionId=<SESSION_ID>"}
```

2. **List Components:**
```bash
curl -X POST "http://localhost:8080/?sessionId=<SESSION_ID>" \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "components/list",
    "params": {}
  }'
```

3. **Execute Component:**
```bash
curl -X POST "http://localhost:8080/?sessionId=<SESSION_ID>" \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "components/execute",
    "params": {
      "component": "add",
      "input": {"a": 5, "b": 3}
    }
  }'
```

## API Reference

### StepflowContext

The `StepflowContext` provides the following methods:

- `putBlob(data: any): Promise<string>` - Store data as a blob and return its ID
- `getBlob(blobId: string): Promise<any>` - Retrieve data by blob ID
- `evaluateFlow(flow: any, input: any): Promise<any>` - Evaluate a nested flow

### Server Registration

```typescript
server.registerComponent<TInput, TOutput>(
  handler: (input: TInput, context?: StepflowContext) => TOutput | Promise<TOutput>,
  name: string,
  options?: {
    description?: string;
    inputSchema?: Record<string, any>;
    outputSchema?: Record<string, any>;
  }
): (input: TInput, context?: StepflowContext) => TOutput | Promise<TOutput>
```

### CLI Options

- `--http` - Run in HTTP mode instead of stdio mode
- `--host <host>` - Host to bind to in HTTP mode (default: `localhost`)
- `--port <port>` - Port to bind to in HTTP mode (default: `8080`)

## Protocol Support

This SDK implements the StepFlow protocol with support for:

- **JSON-RPC 2.0** over stdio or HTTP
- **Methods**: `initialize`, `components/list`, `components/info`, `components/execute`
- **Bidirectional calls**: `blobs/put`, `blobs/get`, `flows/evaluate`
- **Session management** (HTTP mode only)
- **Server-Sent Events** for real-time communication (HTTP mode only)

## Development

### Project Structure

```
src/
├── cli.ts          # Command-line interface
├── http-server.ts  # HTTP server implementation
├── index.ts        # Main exports
├── main.ts         # Entry point
├── protocol.ts     # Protocol type definitions
├── server.ts       # Stdio server implementation
├── transport.ts    # Bidirectional transport layer
└── udf.ts          # User-defined function component
```

### Building

```bash
npm run build
```

### Testing

```bash
npm test
```

### Running Examples

```bash
# Basic stdio server
npm start

# HTTP server
npm start -- --http

# Run example HTTP server demo
npx ts-node examples/http-server-demo.ts
```

### Linting

```bash
npm run lint
```

## Security Considerations

### UDF Component

The UDF component executes user-provided JavaScript code in a sandboxed environment using Node.js's `vm` module:

- Limited built-in functions available (no file system or network access)
- Execution timeout of 30 seconds
- Input validation using JSON Schema
- Function results are cached by blob ID

### HTTP Mode

- CORS is enabled for all origins by default
- Each session is isolated with its own context
- Sessions are automatically cleaned up on disconnect
- Consider implementing authentication/authorization for production use
