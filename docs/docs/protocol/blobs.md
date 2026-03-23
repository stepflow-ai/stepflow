---
sidebar_position: 4
---

# Blob Storage

Stepflow provides content-addressable blob storage for sharing data between components and across workflow steps. Blobs are identified by SHA-256 hashes of their content, enabling deduplication and integrity verification.

## BlobService

The `BlobService` gRPC service exposes two RPCs, also available as REST endpoints:

| RPC | REST | Description |
|-----|------|-------------|
| `PutBlob` | `POST /blobs` | Store data, returns content-based blob ID |
| `GetBlob` | `GET /blobs/{blob_id}` | Retrieve data by blob ID |

Workers access the BlobService via the `STEPFLOW_BLOB_URL` environment variable, which points to the orchestrator's gRPC address (or a dedicated blob pool).

## Blob Types

| Type | Content | Use Case |
|------|---------|----------|
| `data` | JSON (`google.protobuf.Value`) | General data storage (default) |
| `flow` | JSON | Workflow definitions for `eval` |
| `binary` | Raw bytes | PDFs, images, binary files |

## Storing a Blob

**gRPC:** `PutBlob(PutBlobRequest) → PutBlobResponse`

The request uses a `oneof` for content — either `json_data` (Value) or `raw_data` (bytes). Binary blobs can include an optional `filename` and `content_type`.

**REST:** `POST /blobs` supports content negotiation:
- `application/json` — JSON-encoded blob data
- `application/octet-stream` — Raw binary data (with `X-Blob-Filename` header)

**Response** includes the `blob_id` (64-character hex SHA-256 hash):

```
blob_id: "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"
```

## Retrieving a Blob

**gRPC:** `GetBlob(GetBlobRequest) → GetBlobResponse`

**REST:** `GET /blobs/{blob_id}`

Returns the blob content, type, and optional filename/content-type metadata.

## Python SDK Usage

The SDK handles blob operations automatically through `StepflowContext`:

```python
from stepflow_py import StepflowServer, StepflowContext

server = StepflowServer()

@server.component
async def my_component(input: dict, context: StepflowContext) -> dict:
    # Store data as a blob
    blob_id = await context.put_blob({"key": "value"})

    # Retrieve data by blob ID
    data = await context.get_blob(blob_id)

    return {"blob_id": blob_id, "data": data}
```

## Deployment Configuration

Configure the blob API URL in `stepflow-config.yml`:

```yaml
# Local development (default) — orchestrator serves blob endpoints
# No configuration needed

# Kubernetes — explicit URL for workers to reach the orchestrator
blobApi:
  url: "http://orchestrator-service:7840"

# Kubernetes — separate blob service
blobApi:
  enabled: false  # Orchestrator doesn't serve blob endpoints
  url: "http://blob-service:7840"
```

| Field | Default | Description |
|-------|---------|-------------|
| `blobApi.enabled` | `true` | Whether the orchestrator serves blob endpoints |
| `blobApi.url` | (auto) | URL passed to workers for blob operations |
