# HTTP Transport Tests

This directory contains tests for StepFlow's HTTP transport implementation, validating both the streamable HTTP protocol and bidirectional communication capabilities.

## Test Components

The test server (`../test_python_server.py`) provides 6 components:

### Non-Bidirectional Components (simple request/response)
- **`/test/echo`**: Echo with timestamp
- **`/test/math`**: Mathematical operations (add, subtract, multiply, divide)
- **`/test/process_list`**: List processing with prefixes

### Bidirectional Components (using StepflowContext for blob operations)
- **`/test/data_analysis`**: Statistical and distribution analysis with blob storage
- **`/test/chain_processing`**: Multi-step data transformation with intermediate blobs
- **`/test/multi_step_workflow`**: Configurable workflow execution with logging

## Running the Tests

### Manual Server Startup

1. **Start the HTTP test server:**
   ```bash
   cd stepflow-rs
   uv run --project ../sdks/python --extra http python ../tests/test_python_server.py --http --port 8080
   ```

2. **Run the tests (in another terminal):**
   ```bash
   cd stepflow-rs
   TEST_PORT=8080 cargo run -- test ../tests/http/basic_http.yaml --config ../tests/http/stepflow-config.yml
   TEST_PORT=8080 cargo run -- test ../tests/http/bidirectional_http.yaml --config ../tests/http/stepflow-config.yml
   ```

### Automated Server Management (Future)

Once the test framework supports server management, you'll be able to run:

```bash
cd stepflow-rs
cargo run -- test ../tests/http/
```

The test framework will automatically:
1. Allocate random ports to avoid conflicts
2. Start the test servers
3. Run all test cases
4. Stop the servers and clean up

## Test Workflows

### `basic_http.yaml`
Tests non-bidirectional components with simple HTTP request/response patterns:
- Echo functionality with timestamps
- Mathematical operations chaining (add → multiply)
- List processing with custom prefixes
- Basic error handling and edge cases

### `bidirectional_http.yaml`
Tests bidirectional components that use StepflowContext for blob operations:
- Statistical analysis with blob storage
- Data distribution analysis with histograms
- Chain processing with intermediate blob storage
- Complex workflow execution with logging

## Configuration

The `stepflow-config.yml` file configures:

- **HTTP Transport**: Routes `/test/*` components to HTTP server
- **STDIO Transport**: Routes `/test_stdio/*` components to stdio server (for comparison)
- **Built-in Components**: Routes `/builtin/*` to built-in components
- **Port Configuration**: Uses `${TEST_PORT}` environment variable

## Test Server Features

### Server Modes
```bash
# HTTP mode (for transport testing)
python ../tests/test_python_server.py --http --port 8080

# STDIO mode (for comparison testing)  
python ../tests/test_python_server.py
```

### Component Capabilities

**Echo Component:**
```yaml
input: { message: "Hello World" }
output: { echo: "Echo: Hello World", timestamp: "2025-01-15 10:30:00" }
```

**Math Component:**
```yaml
input: { a: 10, b: 5, operation: "add" }
output: { result: 15.0, operation_performed: "10.0 add 5.0 = 15.0" }
```

**Data Analysis Component (Bidirectional):**
```yaml
input: { data: [1,2,3,4,5], analysis_type: "statistics" }
output: { 
  analysis_blob_id: "sha256:abc123...",
  summary: "Statistics for 5 data points: mean=3.00, std_dev=1.41"
}
```

## Architecture Validation

These tests validate:

### HTTP Protocol Compliance
- ✅ **JSON-RPC 2.0**: Proper request/response format
- ✅ **SSE Streaming**: Bidirectional communication via Server-Sent Events
- ✅ **Content Negotiation**: Accept headers for streaming vs direct responses
- ✅ **Error Handling**: HTTP status codes and JSON-RPC errors

### StepFlow Integration  
- ✅ **Blob Operations**: `context.put_blob()` and `/builtin/get_blob`
- ✅ **Component Discovery**: Automatic component registration
- ✅ **Schema Validation**: Input/output type checking
- ✅ **Error Propagation**: Business logic vs system error handling

### Performance Characteristics
- ✅ **Request Latency**: HTTP transport overhead measurement
- ✅ **Blob Storage**: Large data handling via content-addressed storage
- ✅ **Concurrent Requests**: Multiple workflow steps executing simultaneously
- ✅ **Memory Usage**: Efficient data flow between components

## Development

### Adding New Test Components

1. Add component function to `../test_python_server.py`:
   ```python
   @server.component
   def my_component(input: MyInput) -> MyOutput:
       return MyOutput(result="processed")
   ```

2. Update test workflows to use the new component:
   ```yaml
   steps:
     - id: test_my_component
       component: "/test/my_component"
       input: { data: "test" }
   ```

3. Add test cases with expected outputs:
   ```yaml
   test:
     cases:
       - name: "Test my component"
         input: { data: "test" }
         output:
           outcome: success
           result: { result: "processed" }
   ```

### Debugging

Enable detailed HTTP logging:
```bash
RUST_LOG=stepflow_protocol::http=debug cargo run -- test ...
```

The test server logs all component executions and can help identify issues with:
- Request/response serialization
- Blob storage operations  
- Component execution errors
- Transport-level communication

## Comparison Testing

The configuration supports both HTTP and STDIO transports:

```bash
# Test via HTTP transport
TEST_PORT=8080 cargo run -- test basic_http.yaml

# Test via STDIO transport (modify workflow to use /test_stdio/ components)
cargo run -- test basic_stdio.yaml
```

This enables performance and reliability comparisons between transport methods.