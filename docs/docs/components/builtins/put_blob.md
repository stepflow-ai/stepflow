---
sidebar_position: 2
---

### `put_blob`

Store JSON data as content-addressable blobs for efficient reuse across workflow steps.

#### Input

```yaml
input:
  data: <any JSON data>
```

- **`data`** (required): Any JSON data to store as a blob

#### Output

```yaml
output:
  blob_id: "sha256:abc123..."
```

- **`blob_id`**: SHA-256 hash of the stored data

#### Example

```yaml
steps:
  - id: store_user_data
    component: /builtin/put_blob
    input:
      data:
        user_id: { $from: { workflow: input }, path: "user_id" }
        profile: { $from: { step: load_profile } }
        preferences: { $from: { step: load_preferences } }
```