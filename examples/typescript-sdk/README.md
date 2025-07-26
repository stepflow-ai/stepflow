# TypeScript SDK Examples

This directory contains comprehensive examples demonstrating the StepFlow TypeScript SDK's capabilities. These examples showcase the full feature set implemented across four phases of development, including **User-Defined Functions (UDF)**, **HTTP Server Mode**, **Flow Builder API**, and the **Value System**.

## What These Examples Show

- **Complete Component Server**: Full-featured server with multiple component types
- **HTTP Mode**: Server-Sent Events and session management for remote components  
- **Flow Builder API**: Programmatic workflow construction with type safety
- **UDF Integration**: Safe JavaScript execution for custom business logic
- **Value System**: Sophisticated data referencing and protocol compliance

## Requirements

- **Node.js 16+** 
- **npx** or **tsx** for running TypeScript examples

## Running the Examples

```bash
# From this directory
cd examples/typescript-sdk

# Run any example with tsx (recommended)
npx tsx <example-name>.ts
```

## Examples Overview

### 1. Complete Component Server (`complete-component-server.ts`)

**Purpose**: Demonstrates a full-featured StepFlow component server with multiple component types and real-world functionality.

**Features**:
- **Math Components**: Basic arithmetic, advanced calculator, statistical analysis
- **String Processing**: Text transformation, analysis with word frequency
- **Data Processing**: Array operations (filter, map, sort), object transformations
- **Blob Storage**: Store and retrieve data as blobs with metadata
- **UDF Integration**: Execute custom JavaScript code via User-Defined Functions
- **Utility Components**: Health checks, echo testing, random data generation

**Usage**:
```bash
# Run in stdio mode (default)
npx tsx complete-component-server.ts

# Run in HTTP mode
npx tsx complete-component-server.ts --http

# Run on custom host/port
npx tsx complete-component-server.ts --http --host 0.0.0.0 --port 3000
```

**Components Available**:
- `add` - Simple addition
- `calculate` - Multi-operation calculator (add, subtract, multiply, divide, power, mod)
- `statistics` - Statistical analysis (mean, median, variance, etc.)
- `transform_text` - Text transformations (uppercase, lowercase, capitalize, reverse, trim)
- `analyze_text` - Text analysis with optional word frequency
- `process_array` - Array operations with configurable logic
- `transform_object` - Object field manipulation
- `store_blob` - Store data as content-addressed blobs
- `get_blob` - Retrieve blob data by ID
- `udf` - Execute User-Defined Functions
- `execute_code` - Create and execute JavaScript code in one step
- `health_check` - Server health status
- `echo` - Echo messages with optional delay
- `generate_random` - Generate random data of various types

### 2. HTTP Server Demo (`http-server-demo.ts`)

**Purpose**: Demonstrates running the TypeScript SDK in HTTP mode with Server-Sent Events (SSE) for bidirectional communication.

**Features**:
- HTTP-based JSON-RPC communication
- Server-Sent Events for real-time connection management
- MCP-style session negotiation
- Session isolation for multiple clients
- Automatic fallback for non-MCP clients

**Usage**:
```bash
# Start the HTTP server
npx tsx http-server-demo.ts

# In another terminal, connect with SSE to get session ID
curl -N http://localhost:8080/runtime/events

# Use the session ID from the endpoint event to make requests
curl -X POST "http://localhost:8080/?sessionId=<SESSION_ID>" \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"components/list","params":{}}'
```

**Connection Flow**:
1. Client connects to `/runtime/events` SSE endpoint
2. Server sends `endpoint` event with session-specific URL
3. Client uses session URL for all JSON-RPC requests
4. Automatic cleanup when SSE connection closes

### 3. Flow Builder Comprehensive (`flow-builder-comprehensive.ts`)

**Purpose**: Showcases the Flow Builder API for programmatically constructing StepFlow workflows.

**Features**:
- **Workflow Construction**: Programmatic workflow building with fluent API
- **Input/Output Schemas**: Type-safe schema definitions with validation
- **Step References**: Natural property access via JavaScript proxy
- **Data Flow**: Complex data passing between steps
- **Error Handling**: Skip conditions and fallback strategies
- **Flow Analysis**: Introspection and reference tracking
- **YAML Export**: Convert programmatic flows to StepFlow YAML format

**Key Concepts Demonstrated**:
```typescript
// Create a flow builder
const flow = createFlow('user-data-pipeline')
  .describe('A comprehensive pipeline for processing user data')
  .version('1.0.0');

// Add steps with natural property access  
const fetchUser = flow.addStep('fetch_user', '/builtin/http_request');
const validation = flow.addStep('validate_user', '/python/data_validator');

// Reference step outputs naturally
validation.setInput({
  user_data: fetchUser.data.user,  // Proxy-based property access
  validation_rules: input('$.validation.rules')
});

// Export to YAML
const yamlWorkflow = flow.toYaml();
```

**Usage**:
```bash
npx tsx flow-builder-comprehensive.ts
```

**Output**: Shows workflow construction, analysis, and generated YAML format.

### 4. UDF Examples (`udf-examples.ts`)

**Purpose**: Comprehensive demonstration of User-Defined Functions (UDF) capabilities.

**Features**:
- **Simple Expressions**: Basic arithmetic and string operations
- **Multi-line Code**: Complex logic with variables and control flow
- **Named Functions**: Function definitions with proper naming
- **Data Processing**: Array manipulation, object transformation
- **Error Handling**: Try/catch patterns within UDF code
- **Input Validation**: Schema-based input validation
- **Blob Integration**: UDF definitions stored as content-addressed blobs

**UDF Patterns Shown**:

1. **Simple Arithmetic**:
   ```javascript
   input.a + input.b * input.c
   ```

2. **String Processing**:
   ```javascript
   const words = input.text.split(' ');
   const processed = words
     .map(word => word.toLowerCase())
     .filter(word => word.length > input.minLength)
     .join(', ');
   return { original: input.text, filtered: processed };
   ```

3. **Complex Data Transformation**:
   ```javascript
   function transformUserData(input) {
     const { users, config } = input;
     // ... complex transformation logic
     return { users: transformed, statistics: stats };
   }
   ```

4. **Error Handling**:
   ```javascript
   try {
     // processing logic
     return { success: true, result: data };
   } catch (error) {
     return { error: error.message, input: input };
   }
   ```

**Usage**:
```bash
npx tsx udf-examples.ts
```

### 5. Value System Showcase (`value-system-showcase.ts`)

**Purpose**: Demonstrates all features of the Value system for handling data references and literal values in workflows.

**Features**:
- **Literal Values**: Static data with proper escaping using `$literal`
- **Step References**: References to step outputs with JSON path access
- **Workflow Input**: References to workflow input data
- **Property Access**: Natural JavaScript property access via proxy
- **Bracket Notation**: Dynamic property access with `get()` method
- **Complex Structures**: Nested objects with mixed value types
- **Protocol Format**: Conversion to StepFlow's wire format
- **Error Handling**: Skip actions and default values

**Value Types Demonstrated**:

1. **Literal Values**:
   ```typescript
   const message = Value.literal('Hello, World!');
   // Protocol: { "$literal": "Hello, World!" }
   ```

2. **Step References**:
   ```typescript
   const userProfile = Value.step('user_loader', '$.data.profile');
   // Protocol: { "$from": { "step": "user_loader" }, "path": "$.data.profile" }
   ```

3. **Input References**:
   ```typescript
   const userId = input('$.userId');
   // Protocol: { "$from": { "workflow": true }, "path": "$.userId" }
   ```

4. **Property Access via Proxy**:
   ```typescript
   const userStep = StepReference.create('fetch_user');
   const userName = (userStep as any).profile.name;  // Creates reference with path
   ```

5. **Complex Nested Structures**:
   ```typescript
   const config = {
     database: {
       connection: input('$.config.database.url'),
       maxConnections: Value.literal(100),
       timeout: input('$.config.database.timeout')
     },
     pipeline: {
       steps: [
         Value.step('validator'),
         Value.step('transformer', '$.cleaned_data')
       ]
     }
   };
   ```

**Usage**:
```bash
npx tsx value-system-showcase.ts
```

## Architecture Overview

### Phase 1: User-Defined Functions (UDF)
- Safe JavaScript execution using Node.js VM
- Blob-based function storage with content addressing
- Input schema validation with AJV
- Function caching for performance

### Phase 2: HTTP Server Mode  
- Express-based HTTP server with JSON-RPC 2.0
- Server-Sent Events for bidirectional communication
- Session management with unique IDs
- MCP-style connection negotiation
- Fallback compatibility for non-MCP clients

### Phase 3: Flow Builder API
- Fluent API for programmatic workflow construction
- Proxy-based property access for natural syntax
- Type-safe input/output schema definitions
- Reference tracking and flow analysis
- YAML export functionality

### Phase 4: Value System & Protocol Compliance
- Complete value abstraction with proper escaping
- Support for `$literal` and `$from` protocol formats
- JSON Path integration for nested data access
- Error handling with skip actions and defaults
- Full compatibility with StepFlow protocol schema

## Integration with StepFlow Workflows

These examples can be used as **component servers** with StepFlow workflows. Here's how to integrate them:

### 1. Running as a Component Server

```bash
# Start the complete component server in HTTP mode
npx tsx complete-component-server.ts --http --port 8080
```

### 2. StepFlow Configuration

Add to your `stepflow-config.yml`:

```yaml
plugins:
  typescript_server:
    type: stepflow
    transport: http
    url: "http://localhost:8080"

routing:
  - match: "/typescript/*"
    target: typescript_server
  - match: "*"
    target: builtin
```

### 3. Using in Workflows

```yaml
# workflow.yaml
steps:
  - id: process_data
    component: "/typescript/calculate"
    input:
      a: 10
      b: 5 
      operation: "multiply"
  
  - id: transform_text
    component: "/typescript/transform_text"
    input:
      text: "hello world"
      operations: ["uppercase", "reverse"]
```

## Development Tips

1. **Import Paths**: All examples use `../../sdks/typescript/src/index.js` for imports
2. **TypeScript Support**: Run with `npx tsx` for best TypeScript support
3. **Error Handling**: Examples include comprehensive error handling patterns
4. **Testing**: Each example includes validation and test data
5. **Documentation**: All examples are heavily commented with explanations

## Troubleshooting

**Module Resolution Issues**:
- Use `npx tsx` instead of `ts-node` for better ES module support
- Ensure import paths end with `.js` for proper resolution

**Server Connection Issues**:
- Check that ports are available (default: 8080 for HTTP mode)
- Use `--host 0.0.0.0` to bind to all interfaces if needed

**UDF Execution Errors**:
- Verify JavaScript syntax in UDF code blocks
- Check input schema matches provided data
- Review blob storage and retrieval logic

For more information, see the main TypeScript SDK documentation in `../../sdks/typescript/README.md`.