# Python SDK Development Guide

This guide covers Python SDK-specific development for the Stepflow project.

See the root `/CLAUDE.md` for project overview, configuration, and workflow syntax.

## Development Commands

### Testing

```bash
# Test Python SDK with current Python version
uv run --project sdks/python pytest

# Test across all supported versions (3.11, 3.12, 3.13)
./scripts/test-python-versions.sh
```

### Running the SDK

```bash
# Run in stdio mode (default)
uv run --project sdks/python stepflow_worker

# Run in HTTP mode
uv run --project sdks/python --extra http stepflow_worker --http --port 8080

# Run with custom host and port
uv run --project sdks/python --extra http stepflow_worker --http --host 0.0.0.0 --port 8080
```

### Type Generation

```bash
# Regenerate Python types from updated schemas
cd sdks/python
uv run python generate.py

# Check if Python types are up to date without regenerating
uv run python generate.py --check
```

**Important**: Run type generation after Rust schema updates (see `stepflow-rs/CLAUDE.md` for schema generation commands).

## Python Version Compatibility

The Python SDK supports Python 3.11, 3.12, and 3.13. All versions are tested in CI.

### Testing a Specific Python Version

```bash
cd sdks/python

# Install specific Python version
uv python install 3.11  # or 3.12, 3.13

# Pin the version for testing
uv python pin 3.11

# Install dependencies and run tests
uv sync --extra http
uv run poe test
uv run poe typecheck
```

## Transport Modes

The Python SDK supports two transport methods for communication with the Stepflow runtime.

### Stdio Transport (Default)

JSON-RPC over stdio communication with the parent process.

**Usage**:
```bash
# Run in stdio mode
uv run --project sdks/python stepflow_worker

# No additional dependencies required
```

**Configuration** (in stepflow-config.yml):
```yaml
plugins:
  python_stdio:
    type: stepflow
    command: uv
    args: ["--project", "../sdks/python", "run", "stepflow_worker"]

routes:
  "/python/{*component}":
    - plugin: python_stdio
```

**Characteristics**:
- Process-based communication
- Launched by Rust runtime via process spawning
- Simple deployment (no separate server)
- Ideal for local development and single-machine deployments

### HTTP Transport

JSON-RPC over HTTP with Server-Sent Events (SSE) for bidirectional communication.

**Installation**:
```bash
# Install with HTTP support
pip install stepflow-worker[http]

# Or with uv
uv add stepflow-worker[http]
```

**Usage**:
```bash
# Basic HTTP mode
stepflow_worker --http

# Custom host and port
stepflow_worker --http --host 0.0.0.0 --port 8080
```

**Configuration** (in stepflow-config.yml):
```yaml
plugins:
  python_http:
    type: stepflow
    url: "http://localhost:8080"

routes:
  "/python_http/{*component}":
    - plugin: python_http
```

**Features**:
- FastAPI-based HTTP server
- JSON-RPC over HTTP with MCP-style session negotiation
- Server-Sent Events (SSE) for bidirectional communication
- Per-session isolation and context management
- Compatible with existing component registration
- Automatic discovery and component info endpoints
- Backward compatibility with non-MCP clients

**HTTP Server Features**:
1. **Health Endpoint**: `/health` - Returns server status for integration tests
2. **Streamable Transport**: Supports both direct JSON and SSE streaming responses
3. **Accept Header Support**: Requires `Accept: application/json` or `Accept: text/event-stream`
4. **Bidirectional Communication**: Components with `StepflowContext` parameter trigger streaming mode
5. **Session Management**: Automatic context management for bidirectional requests

**Characteristics**:
- Remote component servers
- HTTP-based JSON-RPC communication
- Scalable for distributed deployments
- No process management required by runtime
- Ideal for production and multi-machine deployments

### Session Negotiation (HTTP Transport)

The HTTP transport implements MCP-style session negotiation for proper connection isolation:

**Connection Flow**:
1. Client connects to `/runtime/events` SSE endpoint
2. Server sends an `endpoint` event with session-specific URL: `{"endpoint": "/?sessionId=<uuid>"}`
3. Client uses sessionId URL for all subsequent JSON-RPC requests
4. Each session has isolated context and request handling
5. Sessions automatically cleaned up when SSE connections close

**Fallback Behavior**:
- If no `endpoint` event received within 5 seconds, falls back to direct HTTP communication
- Ensures compatibility with older or non-MCP servers
- Fallback mode uses base URL without session isolation

**Session Benefits**:
- **Isolation**: Each client gets own session context and message handling
- **Reliability**: Request/response matching scoped to individual sessions
- **Scalability**: Multiple clients can connect simultaneously without interference
- **Cleanup**: Resources automatically freed when clients disconnect

## Component Development

### Basic Component Registration

```python
from stepflow_worker import StepflowStdioServer
import msgspec

class MyInput(msgspec.Struct):
    message: str
    count: int

class MyOutput(msgspec.Struct):
    result: str

server = StepflowStdioServer()

@server.component
def my_component(input: MyInput) -> MyOutput:
    result = input.message * input.count
    return MyOutput(result=result)

server.run()
```

### Using StepflowContext for Bidirectional Communication

Components can receive a `StepflowContext` parameter to access runtime operations:

```python
from stepflow_worker import StepflowStdioServer, StepflowContext
import msgspec

class MyInput(msgspec.Struct):
    data: dict

class MyOutput(msgspec.Struct):
    blob_id: str

server = StepflowStdioServer()

@server.component
async def my_component(input: MyInput, context: StepflowContext) -> MyOutput:
    # Store data as a blob
    blob_id = await context.put_blob(input.data)

    # Retrieve blob data
    retrieved_data = await context.get_blob(blob_id)

    return MyOutput(blob_id=blob_id)

server.run()
```

**Available Context Methods**:
- `put_blob(data: dict) -> str`: Store JSON data as blob, returns content-based ID
- `get_blob(blob_id: str) -> dict`: Retrieve JSON data by blob ID

### HTTP Server Components

The Python SDK uses HTTP transport by default:

```python
from stepflow_worker import StepflowServer, StepflowContext
import msgspec

class MyInput(msgspec.Struct):
    data: dict

class MyOutput(msgspec.Struct):
    blob_id: str

server = StepflowServer()

@server.component
async def my_component(input: MyInput, context: StepflowContext) -> MyOutput:
    # Store data as a blob
    blob_id = await context.put_blob(input.data)
    return MyOutput(blob_id=blob_id)

if __name__ == "__main__":
    import argparse
    import asyncio

    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=0)
    args = parser.parse_args()

    asyncio.run(server.run(host=args.host, port=args.port))
```

## Architecture Notes

The Python SDK has undergone significant architectural improvements:

1. **Unified Core Server**: Both HTTP and STDIO servers delegate to `StepflowServer`
2. **Context Inheritance**: `StepflowStreamingContext` inherits from `StepflowContext`
3. **MessageDecoder Consolidation**: Single request/response correlation system
4. **Clean Test Separation**: Core functionality vs transport layer testing

This architecture ensures consistent behavior across transport modes while maintaining clean separation of concerns.

## Testing Best Practices

- Test components in both stdio and HTTP modes when applicable
- Use type checking with `poe typecheck` to catch type errors early
- Run tests across all supported Python versions before submitting changes
- Mock `StepflowContext` for unit testing components without runtime dependencies
