---
sidebar_position: 3
---

# HTTP Transport

The HTTP transport enables network-based communication between the Stepflow runtime and component servers using HTTP protocol with optional MCP-style session negotiation. This transport is ideal for distributed deployments, microservices architectures, and scenarios requiring horizontal scaling.

## Overview

HTTP transport communicates with component servers running as independent HTTP services:
- **HTTP requests**: JSON-RPC messages sent via POST requests
- **HTTP responses**: JSON-RPC responses returned as HTTP response bodies
- **Server-Sent Events (SSE)**: Optional bidirectional communication channel
- **Session isolation**: MCP-style session negotiation for multi-client scenarios

## Basic HTTP Communication

### Request Format
```http
POST /api/jsonrpc HTTP/1.1
Host: localhost:8080
Content-Type: application/json; charset=utf-8
Content-Length: 123

{"jsonrpc":"2.0","id":"uuid-123","method":"components/list","params":{}}
```

### Response Format
```http
HTTP/1.1 200 OK
Content-Type: application/json; charset=utf-8
Content-Length: 89

{"jsonrpc":"2.0","id":"uuid-123","result":{"components":[{"name":"processor"}]}}
```

### Protocol Configuration
See [Configuration Documentation](../../configuration/) for complete HTTP transport configuration options.