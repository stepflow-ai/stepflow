---
sidebar_position: 3
---

### `get_blob`

Retrieve JSON data from previously stored blobs.

#### Input

```yaml
input:
  blob_id: "sha256:abc123..."
```

- **`blob_id`** (required): Content hash of the blob to retrieve

#### Output

```yaml
output:
  data: <stored JSON data>
```

- **`data`**: The original JSON data that was stored

#### Example

```yaml
steps:
  - id: retrieve_user_data
    component: /builtin/get_blob
    input:
      blob_id: { $step: store_user_data, path: "blob_id" }
```

#### Use Cases

- **Large Data Reuse**: Store large datasets once and reference them multiple times
- **User-Defined Functions**: Store Python code and reuse across steps
- **Configuration Storage**: Store complex configuration objects
- **Intermediate Results**: Cache expensive computation results