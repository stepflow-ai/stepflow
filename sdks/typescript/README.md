# Stepflow TypeScript SDK

A comprehensive TypeScript SDK for creating Stepflow component plugins and building workflows programmatically. This SDK provides complete feature parity with the Python SDK, including support for bidirectional communication, blob operations, user-defined functions (UDF), HTTP server mode, and a powerful Flow Builder API.

## 🚀 Features

### Component Development
- 🔧 **User-Defined Functions (UDF)** - Execute dynamic JavaScript code stored as blobs
- 💾 **Blob Operations** - Store and retrieve data using content-based SHA-256 addressing
- 🔄 **Bidirectional Communication** - Components can make requests back to the Stepflow runtime
- 🏗️ **Type-Safe Development** - Full TypeScript support with comprehensive type inference

### Server Modes
- 📡 **Stdio Mode** - Traditional stdin/stdout JSON-RPC communication
- 🌐 **HTTP Server Mode** - REST API with Server-Sent Events for real-time communication
- 🔌 **Session Isolation** (HTTP mode) - Each client gets isolated session context
- 🔒 **MCP-Style Negotiation** - Proper connection management and cleanup

### Workflow Building
- 🏗️ **Flow Builder API** - Programmatically create workflows with fluent syntax
- 🔗 **Value System** - Advanced reference system for step outputs and workflow inputs
- 📊 **JSON Path Support** - Access nested data with `$.field.nested[0]` syntax
- ⚡ **Proxy Magic** - Natural property access like `step.result.data`
- 🛡️ **Error Handling** - Comprehensive error strategies (retry, skip, fail, default)

### Protocol Compliance
- 📋 **Schema Compliant** - Generates correct `$from`/`$literal` protocol format
- 🔄 **Bidirectional JSON-RPC** - Full protocol support with method calls in both directions
- 🆔 **Content-Based IDs** - Blob storage with SHA-256 content addressing

## Installation

```bash
npm install stepflow-py
```

## Quick Start

### Basic Component Server

```typescript
import { StepflowStdioServer } from 'stepflow-py';

const server = new StepflowStdioServer();

// Simple math component
server.registerComponent(
  (input: { a: number; b: number }): { result: number } => {
    return { result: input.a + input.b };
  },
  'add',
  {
    description: 'Adds two numbers together',
    inputSchema: {
      type: 'object',
      properties: {
        a: { type: 'number' },
        b: { type: 'number' }
      },
      required: ['a', 'b']
    }
  }
);

server.run();
```

### HTTP Server Mode

```bash
# Start HTTP server
npx ts-node src/main.ts --http --port 8080

# Or with custom configuration
npx ts-node src/main.ts --http --host 0.0.0.0 --port 3000
```

## Flow Builder API

Create workflows programmatically with a fluent, type-safe API:

### Basic Flow Construction

```typescript
import { createFlow, Value, input, OnError } from 'stepflow-py';

// Create a new workflow
const flow = createFlow('data-processing', 'Process user data with validation')
  .setInputSchema({
    type: 'object',
    properties: {
      userId: { type: 'string' },
      config: {
        type: 'object',
        properties: {
          database: { type: 'string' },
          retries: { type: 'number' }
        }
      }
    },
    required: ['userId']
  });

// Add steps with input references
const fetchUser = flow.addStep({
  id: 'fetch_user',
  component: '/api/get_user',
  input: {
    id: input('$.userId'),                    // Reference to workflow input
    db_url: input('$.config.database')       // Nested input reference
  },
  onError: OnError.retry(3)
});

const processData = flow.addStep({
  id: 'process_data',
  component: '/python/transform',
  input: {
    user: fetchUser.ref,                     // Reference to entire step output
    name: fetchUser.data.name,               // Property access via proxy
    settings: fetchUser.get('preferences')   // Bracket notation access
  }
});

// Set workflow output
flow.setOutput({
  processed: processData.ref,
  summary: Value.literal('Processing completed'),
  userId: input('$.userId')
});

// Build the workflow
const workflow = flow.build();
console.log(JSON.stringify(workflow, null, 2));
```

### Advanced Value System

The Value system provides powerful ways to reference data:

```typescript
import { Value, input } from 'stepflow-py';

// Literal values (escaped in protocol)
const config = {
  retries: Value.literal(3),
  timeout: Value.literal('30s'),
  settings: Value.literal({ debug: true })
};

// Input references with JSON Path
const userRefs = {
  id: input('$.userId'),
  name: input('$.user.profile.name'),
  email: input('$.user.contact.email'),
  preferences: input('$.user.settings')
};

// Step references with paths
const stepRefs = {
  result: Value.step('processor'),              // Entire step output
  status: Value.step('validator', '$.status'),  // Nested field
  errors: Value.step('validator', '$.errors[0]') // Array access
};

// Chained property access
const chainedRefs = {
  userName: input().user.profile.name,          // Via proxy
  firstError: step.validation.errors.get(0),   // Mixed syntax
  dbConfig: input().get('config').get('database') // Bracket notation
};
```

### Error Handling Strategies

```typescript
import { OnError } from 'stepflow-py';

// Different error handling approaches
flow.addStep({
  id: 'risky_operation',
  component: '/external/api',
  input: { data: 'test' },
  onError: OnError.retry(5)  // Retry up to 5 times
});

flow.addStep({
  id: 'optional_step',
  component: '/optional/process',
  input: { data: 'test' },
  onError: OnError.skip()  // Skip on error
});

flow.addStep({
  id: 'critical_step',
  component: '/critical/process',
  input: { data: 'test' },
  onError: OnError.fail()  // Fail entire workflow
});

flow.addStep({
  id: 'with_fallback',
  component: '/may/fail',
  input: { data: 'test' },
  onError: OnError.default({ fallback: 'default_value' })  // Use default
});
```

### Skip Conditions

```typescript
// Conditional step execution
const validator = flow.addStep({
  id: 'validate',
  component: '/validation/check',
  input: { data: input('$.payload') }
});

flow.addStep({
  id: 'conditional_process',
  component: '/process/data',
  input: { data: input('$.payload') },
  skipIf: validator.shouldSkip  // Skip if validator.shouldSkip is true
});
```

### Flow Analysis

```typescript
// Analyze workflow structure
const references = flow.getReferences();
const stepRefs = references.filter(ref => ref instanceof StepReference);
const inputRefs = references.filter(ref => ref instanceof WorkflowInput);

console.log(`Found ${stepRefs.length} step references`);
console.log(`Found ${inputRefs.length} input references`);

// Analyze individual steps
const step = flow.step('process_data');
const stepReferences = step?.getReferences() || [];
console.log(`Step has ${stepReferences.length} references`);
```

## User-Defined Functions (UDF)

Execute dynamic JavaScript code stored as blobs:

### UDF Component Setup

```typescript
import { StepflowStdioServer, udf, udfSchema } from 'stepflow-py';

const server = new StepflowStdioServer();

// Register the built-in UDF component
server.registerComponent(udf, 'udf', udfSchema.component);

server.run();
```

### UDF Blob Format

Store JavaScript code as blobs with this structure:

```json
{
  "code": "input.x * input.y + input.z",
  "input_schema": {
    "type": "object",
    "properties": {
      "x": { "type": "number" },
      "y": { "type": "number" },
      "z": { "type": "number" }
    },
    "required": ["x", "y", "z"]
  }
}
```

### UDF Code Patterns

#### 1. Simple Expressions
```javascript
"input.price * input.quantity"
```

#### 2. Multi-line Code
```javascript
`const total = input.items.reduce((sum, item) => sum + item.price, 0);
const tax = total * input.taxRate;
return { total, tax, grandTotal: total + tax };`
```

#### 3. Named Functions
```json
{
  "function_name": "calculateDiscount",
  "code": `
    function calculateDiscount(input) {
      const { price, discountPercent, memberLevel } = input;
      const baseDiscount = price * (discountPercent / 100);
      const memberBonus = memberLevel === 'premium' ? baseDiscount * 0.1 : 0;
      return {
        discount: baseDiscount + memberBonus,
        finalPrice: price - baseDiscount - memberBonus
      };
    }
  `
}
```

### UDF Security Features

- ✅ **Sandboxed Execution** - Uses Node.js `vm` module for isolation
- ✅ **No File System Access** - Limited built-in functions only
- ✅ **Execution Timeout** - 30-second timeout prevents infinite loops
- ✅ **Input Validation** - JSON Schema validation of inputs
- ✅ **Result Caching** - Functions cached by blob ID for performance

## Blob Operations

Components can store and retrieve data using content-based addressing:

```typescript
import { StepflowStdioServer, StepflowContext } from 'stepflow-py';

const server = new StepflowStdioServer();

// Component that stores large data as blobs
server.registerComponent(
  async (input: { largeData: any }, context?: StepflowContext) => {
    if (!context) throw new Error('Context required');

    // Store data as blob - returns SHA-256 hash
    const blobId = await context.putBlob(input.largeData);

    return {
      blobId,
      message: 'Data stored successfully',
      size: JSON.stringify(input.largeData).length
    };
  },
  'store_large_data'
);

// Component that processes blob data
server.registerComponent(
  async (input: { blobId: string }, context?: StepflowContext) => {
    if (!context) throw new Error('Context required');

    // Retrieve data by blob ID
    const data = await context.getBlob(input.blobId);

    // Process the data
    const processed = data.map((item: any) => ({
      ...item,
      processed: true,
      timestamp: new Date().toISOString()
    }));

    return { processed };
  },
  'process_blob_data'
);

server.run();
```

## HTTP Server Mode

### Server-Sent Events (SSE) Connection

The HTTP server uses SSE for real-time bidirectional communication:

```bash
# Connect to get session ID
curl -N http://localhost:8080/runtime/events

# Server responds with:
# event: endpoint
# data: {"endpoint":"/?sessionId=abc-123-def"}
```

### JSON-RPC API Calls

```bash
# List available components
curl -X POST "http://localhost:8080/?sessionId=abc-123-def" \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 1,
    "method": "components/list",
    "params": {}
  }'

# Execute a component
curl -X POST "http://localhost:8080/?sessionId=abc-123-def" \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "id": 2,
    "method": "components/execute",
    "params": {
      "component": "add",
      "input": {"a": 10, "b": 20}
    }
  }'
```

### Session Management

- 🔒 **Automatic Isolation** - Each SSE connection gets unique session
- 🧹 **Auto Cleanup** - Sessions cleaned up when SSE connection closes
- 🔄 **Fallback Support** - Works with non-MCP clients
- ⏱️ **Connection Timeout** - 5-second negotiation timeout

## Command Line Interface

```bash
# Stdio mode (default)
npx stepflow-py

# HTTP server mode
npx stepflow-py --http

# Custom host and port
npx stepflow-py --http --host 0.0.0.0 --port 3000

# Show help
npx stepflow-py --help
```

## API Reference

### StepflowContext

Available in component handlers for runtime operations:

```typescript
interface StepflowContext {
  putBlob(data: any): Promise<string>;           // Store blob, returns SHA-256 ID
  getBlob(blobId: string): Promise<any>;         // Retrieve blob by ID
  evaluateFlow(flow: any, input: any): Promise<any>; // Execute nested workflow
}
```

### Value System Types

```typescript
// Core value types
class Value {
  static literal(value: any): Value;              // Escaped literal value
  static step(stepId: string, path?: string): Value;  // Step reference
  static input(path?: string): Value;             // Workflow input reference

  get(key: string | number): Value;               // Bracket notation access
  toValueTemplate(): ValueTemplate;               // Convert to protocol format
}

// Convenience functions
function input(path?: string): Value;             // Create input reference
function createFlow(name?: string): FlowBuilder;  // Create flow builder
```

### Flow Builder API

```typescript
class FlowBuilder {
  // Configuration
  setInputSchema(schema: Schema): FlowBuilder;
  setOutputSchema(schema: Schema): FlowBuilder;
  setOutput(output: Valuable): FlowBuilder;

  // Step management
  addStep(options: {
    id: string;
    component: string;
    input?: Valuable;
    inputSchema?: Schema;
    outputSchema?: Schema;
    skipIf?: Valuable;
    onError?: OnErrorAction;
  }): StepHandle;

  step(stepId: string): StepHandle | undefined;

  // Analysis
  getReferences(): Array<StepReference | WorkflowInput>;
  getStepIds(): string[];

  // Build
  build(): Flow;
}

// Error handling helpers
class OnError {
  static fail(): OnErrorAction;
  static skip(): OnErrorAction;
  static retry(maxAttempts?: number): OnErrorAction;
  static default(value: any): OnErrorAction;
}
```

## Protocol Format

The SDK generates protocol-compliant ValueTemplate structures:

### Step References
```json
{
  "$from": { "step": "stepId" },
  "path": "$.result.data"
}
```

### Workflow Input References
```json
{
  "$from": { "workflow": "input" },
  "path": "$.user.name"
}
```

### Escaped Literals
```json
{
  "$literal": "This won't be expanded as a reference"
}
```

### Complex Nested Structures
```json
{
  "config": {
    "database": {
      "$from": { "workflow": "input" },
      "path": "$.db_config"
    },
    "retries": { "$literal": 3 }
  },
  "data": {
    "$from": { "step": "processor" },
    "path": "$.output"
  }
}
```

## Examples

Check out the `/examples` directory for:

- 📝 **Basic Components** - Simple math and string operations
- 🌐 **HTTP Server Demo** - Complete HTTP server setup
- 🏗️ **Flow Builder Examples** - Workflow construction patterns
- 💾 **Blob Storage Examples** - Data persistence patterns
- 🔧 **UDF Examples** - Dynamic code execution
- 🔄 **Bidirectional Communication** - Runtime callbacks

## Development

### Project Structure

```
src/
├── cli.ts           # Command-line interface
├── flow-builder.ts  # Flow Builder API
├── http-server.ts   # HTTP server implementation
├── index.ts         # Main exports
├── main.ts          # Entry point
├── protocol.ts      # Protocol type definitions
├── server.ts        # Stdio server implementation
├── transport.ts     # Bidirectional transport layer
├── udf.ts          # User-defined function component
└── value.ts        # Value system and references
```

### Commands

```bash
# Development
npm run build        # Compile TypeScript
npm test            # Run test suite
npm run lint        # Check code style
npm start           # Run in stdio mode
npm start -- --http # Run in HTTP mode

# Testing
npm test            # All tests
npm run test:watch  # Watch mode
npm run test:coverage # Coverage report
```

### Building and Publishing

```bash
# Build for production
npm run build

# Verify build
node dist/main.js --help

# Run built version
node dist/main.js --http --port 8080
```

## TypeScript Support

Full TypeScript support with:

- 🏷️ **Generic Components** - Type-safe input/output interfaces
- 🔍 **Type Inference** - Automatic type detection in Flow Builder
- 📋 **Schema Integration** - JSON Schema to TypeScript types
- 🛡️ **Compile-time Safety** - Catch errors before runtime
- 📚 **Comprehensive Types** - Exported types for all APIs

```typescript
// Fully typed component
interface MyInput {
  userId: string;
  options?: {
    include?: string[];
    format?: 'json' | 'xml';
  };
}

interface MyOutput {
  user: {
    id: string;
    name: string;
    email: string;
  };
  metadata: {
    retrieved: string;
    format: string;
  };
}

server.registerComponent<MyInput, MyOutput>(
  async (input, context) => {
    // TypeScript knows the shape of input and expected output
    const user = await fetchUser(input.userId);
    return {
      user: {
        id: user.id,
        name: user.fullName,
        email: user.emailAddress
      },
      metadata: {
        retrieved: new Date().toISOString(),
        format: input.options?.format || 'json'
      }
    };
  },
  'get_user'
);
```

## Security Considerations

### UDF Execution
- ✅ Sandboxed in Node.js `vm` module
- ✅ No file system or network access
- ✅ 30-second execution timeout
- ✅ Input validation with JSON Schema
- ✅ Function result caching by blob ID

### HTTP Server
- ⚠️ CORS enabled for all origins (configure for production)
- ✅ Session isolation prevents cross-client data leaks
- ✅ Automatic session cleanup on disconnect
- ⚠️ No built-in authentication (add your own middleware)
- ✅ Request/response validation

### Blob Storage
- ✅ Content-based addressing with SHA-256
- ✅ Automatic deduplication
- ✅ JSON serialization validation
- ⚠️ No encryption at rest (add your own if needed)

---

## License

MIT License - see LICENSE file for details.

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## Support

- 📖 [Documentation](https://docs.stepflow.ai)
- 🐛 [Issue Tracker](https://github.com/stepflow/stepflow/issues)
- 💬 [Discussions](https://github.com/stepflow/stepflow/discussions)
- 📧 [Email Support](mailto:support@stepflow.ai)