# Pingora Load Balancer Design for Stepflow

## Overview

This load balancer provides instance-aware routing for Stepflow component servers using the Streamable HTTP transport pattern. It supports both simple request/response and bidirectional streaming communication patterns.

## Streamable HTTP Transport Protocol

### Protocol Overview

**Important**: Stepflow uses the **Streamable HTTP transport pattern** (inspired by MCP's transport design) but with **Stepflow's JSON-RPC protocol**, not MCP's protocol. The transport mechanism (HTTP POST with optional SSE streaming) is similar, but the JSON-RPC methods and message formats are Stepflow-specific.

The Streamable HTTP transport uses a **per-request** streaming model:

1. **Client sends POST request** with Stepflow JSON-RPC method call
2. **Server responds** based on component requirements:
   - **Simple components**: `Content-Type: application/json` with immediate JSON response
   - **Bidirectional components** (with `StepflowContext` parameter): `Content-Type: text/event-stream` with SSE stream

### Request Flow Examples

#### Simple (Non-Bidirectional) Component

```
POST /component/double HTTP/1.1
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "id": "req-1",
  "method": "execute",
  "params": {
    "component": "double",
    "input": {"value": 5}
  }
}

→ Response:

HTTP/1.1 200 OK
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "id": "req-1",
  "result": {"result": 10}
}
```

#### Bidirectional Component (with SSE stream)

```
POST /component/process_with_context HTTP/1.1
Content-Type: application/json

{
  "jsonrpc": "2.0",
  "id": "req-2",
  "method": "execute",
  "params": {
    "component": "process_with_context",
    "input": {"data": "..."}
  }
}

→ Response:

HTTP/1.1 200 OK
Content-Type: text/event-stream
Transfer-Encoding: chunked

event: message
data: {"jsonrpc": "2.0", "id": "server-req-1", "method": "blob_store", "params": {...}}

event: message
data: {"jsonrpc": "2.0", "id": "server-req-2", "method": "blob_get", "params": {...}}

event: result
data: {"jsonrpc": "2.0", "id": "req-2", "result": {...}}
```

**Client must respond to server requests**:
```
POST / HTTP/1.1
Content-Type: application/json
Stepflow-Instance-Id: component-server-abc-12345678

{
  "jsonrpc": "2.0",
  "id": "server-req-1",
  "result": {"blobId": "blob-xyz"}
}
```

### Key Protocol Characteristics

1. **No persistent SSE connection** - Each POST may or may not return SSE
2. **Server decides streaming** - Based on component signature (has `StepflowContext` parameter)
3. **Bidirectional via separate POSTs** - Client responses are new HTTP POST requests
4. **Correlation via JSON-RPC IDs** - Request/response matching uses `id` field
5. **Instance affinity required** - Responses must route back to same instance

## Instance-Based Routing Design

### Why Instance Routing?

Bidirectional components maintain **in-memory state** for pending requests:
- Request ID → pending future
- Cannot route responses to different instance
- State is lost on pod restart (cannot resume)

### Instance ID Design

**Format**: `{pod-name}-{startup-uuid-8chars}`

**Example**: `component-server-abc123-5f3a2b1c`

**Assignment**:
```python
import os
import uuid

POD_NAME = os.getenv("POD_NAME", "local")
STARTUP_UUID = uuid.uuid4().hex[:8]
INSTANCE_ID = f"{POD_NAME}-{STARTUP_UUID}"
```

**Properties**:
- Unique per pod startup
- Correlates with k8s pod name (for debugging)
- Changes on restart (clear state boundary)
- Not resumable (in-memory state is ephemeral)

### Header-Based Routing

**Headers**:
```
Stepflow-Instance-Id: component-server-abc-12345678
```

**Usage**:
- **Initial request**: No instance ID (load balancer chooses)
- **Server includes in response**: Via response header or SSE event
- **Client includes in subsequent requests**: For responses to server requests

### Connection Flow

#### 1. Initial Component Execution (No Instance Preference)

```
Client → Load Balancer → [Round Robin] → Instance A

POST /component/execute
(no Stepflow-Instance-Id header)

Instance A responds:
  Content-Type: text/event-stream
  Stepflow-Instance-Id: pod-a-12345678

  event: message
  data: {"id": "server-req-1", "method": "blob_store", ...}
```

#### 2. Client Responds to Server Request (Must Route to Instance A)

```
Client → Load Balancer → Instance A

POST /
Stepflow-Instance-Id: pod-a-12345678

{
  "id": "server-req-1",
  "result": {...}
}
```

**Load balancer routing logic**:
- Sees `Stepflow-Instance-Id: pod-a-12345678`
- Routes to Instance A (or 503 if not available)

### Backend Discovery

Load balancer learns instance IDs via **health endpoint polling**:

```
GET /health HTTP/1.1

→ Response:

{
  "status": "healthy",
  "instanceId": "component-server-abc-12345678"
}
```

**Discovery process**:
1. Load balancer queries k8s Service for endpoints
2. Polls `/health` on each endpoint
3. Builds routing table: `instance_id → backend_address`
4. Updates every 10-30 seconds

## Load Balancer Implementation

### Routing Logic

```rust
fn select_backend(request: &Request, backends: &[Backend]) -> Result<&Backend> {
    // Check for explicit instance routing
    if let Some(instance_id) = request.headers().get("Stepflow-Instance-Id") {
        let instance_id = instance_id.to_str()?;

        // Find backend with matching instance ID
        return backends.iter()
            .find(|b| b.instance_id == instance_id && b.healthy)
            .ok_or(Error::InstanceNotAvailable(instance_id.to_string()));
    }

    // No instance preference - use load balancing algorithm
    least_connections_backend(backends)
}
```

### SSE Stream Handling

**Critical requirement**: **No buffering** of SSE streams

**Implementation**:
```rust
async fn proxy_request(
    session: &mut Session,
    backend: &Backend,
) -> Result<()> {
    // Connect to backend
    let mut backend_stream = connect_to_backend(backend).await?;

    // Check response Content-Type
    let content_type = backend_stream.response_header()
        .headers
        .get("content-type")
        .and_then(|v| v.to_str().ok());

    match content_type {
        Some("text/event-stream") => {
            // SSE stream - pass through without buffering
            session.write_response_header(backend_stream.response_header().clone()).await?;

            // Stream body chunks directly
            while let Some(chunk) = backend_stream.read_body_bytes().await? {
                session.write_body_bytes(chunk).await?;
            }
        }
        _ => {
            // Regular response - can buffer if needed
            session.write_response(backend_stream.response_header().clone()).await?;
            copy_body(&mut backend_stream, session).await?;
        }
    }

    Ok(())
}
```

**Key points**:
- Detect `Content-Type: text/event-stream`
- Stream body chunks directly (no buffering)
- Use chunked transfer encoding
- Maintain connection until stream closes

### Error Handling

#### Instance Not Found (503)

When `Stepflow-Instance-Id` header points to unavailable instance:

```
HTTP/1.1 503 Service Unavailable
Retry-After: 5
Content-Type: application/json

{
  "error": {
    "code": -32000,
    "message": "Instance not available",
    "data": {
      "instanceId": "component-server-abc-12345678",
      "reason": "Instance not found in healthy backends"
    }
  }
}
```

**Client behavior**:
- Detect 503 for instance routing
- Abandon pending requests for that instance
- Retry initial component execution (will get new instance)

#### Backend Unhealthy

Health check failures:
- Mark backend as unhealthy
- Remove from routing table
- Return 503 for new requests
- Existing connections may complete

### Load Balancing Algorithms

**For initial requests (no instance ID)**:

1. **Least Connections** (Recommended)
   - Track active connection count per backend
   - Route to backend with fewest connections
   - Good for long-lived SSE streams

2. **Round Robin** (Alternative)
   - Simple rotation
   - Works well for uniform request distribution
   - Simpler implementation

**Connection counting**:
- Increment on new request
- Decrement on response complete
- SSE streams count as 1 connection until closed

## Component Server Changes

### Instance ID Exposure

#### Startup
```python
import os
import uuid

POD_NAME = os.getenv("POD_NAME", "local")
INSTANCE_ID = f"{POD_NAME}-{uuid.uuid4().hex[:8]}"

logger.info(f"Component server starting with instance ID: {INSTANCE_ID}")
```

#### Health Endpoint
```python
@app.get("/health")
def health():
    return {
        "status": "healthy",
        "instanceId": INSTANCE_ID,
        "startupTime": STARTUP_TIME.isoformat(),
    }
```

#### Response Headers (for bidirectional components)
```python
@app.post("/")
async def handle_request(request: Request):
    # Check if component requires bidirectional communication
    if requires_bidirectional(component):
        # Return SSE stream with instance ID header
        return StreamingResponse(
            generate_sse_events(),
            media_type="text/event-stream",
            headers={
                "Stepflow-Instance-Id": INSTANCE_ID,
                "Cache-Control": "no-cache",
                "X-Accel-Buffering": "no",  # Disable nginx buffering
            }
        )
    else:
        # Simple response
        return JSONResponse(result)
```

## Stepflow Runtime Changes

### HTTP Client Session Management

```rust
pub struct ComponentHttpClient {
    base_url: String,
    client: reqwest::Client,
    instance_id: Option<String>,  // Track instance for this session
}

impl ComponentHttpClient {
    pub async fn execute_component(
        &mut self,
        component: &str,
        input: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let mut request = self.client
            .post(&self.base_url)
            .json(&json!({
                "jsonrpc": "2.0",
                "id": generate_request_id(),
                "method": "execute",
                "params": {
                    "component": component,
                    "input": input,
                }
            }));

        // Include instance ID if we have one (for subsequent requests)
        if let Some(instance_id) = &self.instance_id {
            request = request.header("Stepflow-Instance-Id", instance_id);
        }

        let response = request.send().await?;

        // Check if response is SSE stream
        if response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            == Some("text/event-stream")
        {
            // Extract instance ID from response header
            if let Some(instance_id) = response.headers().get("Stepflow-Instance-Id") {
                self.instance_id = Some(instance_id.to_str()?.to_string());
            }

            // Handle SSE stream with bidirectional communication
            self.handle_sse_stream(response).await
        } else {
            // Simple JSON response
            let result: serde_json::Value = response.json().await?;
            Ok(result)
        }
    }

    async fn handle_sse_stream(
        &mut self,
        response: reqwest::Response,
    ) -> Result<serde_json::Value> {
        let mut stream = response.bytes_stream();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            let events = parse_sse_events(&chunk)?;

            for event in events {
                match event {
                    SseEvent::Message(request) => {
                        // Server sent a request (e.g., blob_store)
                        let response = self.handle_server_request(request).await?;

                        // Send response back with instance ID
                        self.send_response(response).await?;
                    }
                    SseEvent::Result(result) => {
                        // Final result
                        return Ok(result);
                    }
                }
            }
        }

        Err(Error::UnexpectedStreamEnd)
    }

    async fn send_response(&self, response: JsonRpcResponse) -> Result<()> {
        let mut request = self.client.post(&self.base_url).json(&response);

        // CRITICAL: Include instance ID for routing
        if let Some(instance_id) = &self.instance_id {
            request = request.header("Stepflow-Instance-Id", instance_id);
        }

        request.send().await?;
        Ok(())
    }
}
```

## Configuration

### Pingora Load Balancer Config

```yaml
# pingora-config.yaml
upstream:
  service: component-server.stepflow-demo.svc.cluster.local
  port: 8080
  health_check:
    path: /health
    interval: 10s
    timeout: 2s

load_balancing:
  algorithm: least_connections

server:
  listen: 0.0.0.0:8080
  threads: 4
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: pingora-lb
spec:
  replicas: 2
  template:
    spec:
      containers:
      - name: pingora
        image: localhost:5000/pingora-lb:latest
        ports:
        - containerPort: 8080
        env:
        - name: UPSTREAM_SERVICE
          value: component-server.stepflow-demo.svc.cluster.local:8080
```

## Testing Strategy

### Unit Tests
- Header parsing and extraction
- Backend selection logic
- Instance ID matching
- Error handling (503 responses)

### Integration Tests
1. **Simple component** - Verify pass-through
2. **Bidirectional component** - Verify instance routing
3. **Pod failure** - Verify 503 and reconnection
4. **Multiple instances** - Verify load distribution
5. **SSE streaming** - Verify no buffering (test with large streams)

### Load Tests
- 100 concurrent bidirectional components
- Verify instance affinity maintained
- Measure latency overhead
- Test pod scaling during load

## Monitoring and Debugging

### Metrics

- Request count by instance ID
- Active connections per instance
- 503 errors (instance not found)
- Backend health check failures
- Request routing decisions (with/without instance ID)

### Logging

```
INFO: Request routed to instance component-server-abc-12345678 (explicit)
INFO: Request load-balanced to instance component-server-def-87654321 (least-conn)
WARN: Instance component-server-xyz-11111111 not found, returning 503
ERROR: Backend component-server-abc-12345678 health check failed
```

### Debug Headers

Include debug info in responses:
```
X-Stepflow-Routed-Instance: component-server-abc-12345678
X-Stepflow-Backend-Address: 10.42.0.5:8080
X-Stepflow-Routing-Decision: instance-header
```

## Future Enhancements

1. **Graceful pod termination** - Drain connections before shutdown
2. **Circuit breakers** - Fast-fail for unhealthy backends
3. **Metrics endpoint** - Prometheus metrics at `/metrics`
4. **Request tracing** - OpenTelemetry integration
5. **Rate limiting** - Per-instance or per-tenant limits
6. **Geographic routing** - Multi-region support
