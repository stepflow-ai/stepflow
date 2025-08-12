---
sidebar_position: 5
---

### `load_file`

Load and parse files from the filesystem with automatic format detection.

#### Input

```yaml
input:
  path: "path/to/file.json"
  format: "json"  # optional: "json", "yaml", "text"
```

- **`path`** (required): File path to load
- **`format`** (optional): Force specific format, otherwise auto-detected from extension

#### Output

```yaml
output:
  data: <parsed file content>
  metadata:
    resolved_path: "/absolute/path/to/file.json"
    size_bytes: 1024
    format: "json"
```

- **`data`**: Parsed file content (JSON object, YAML object, or text string)
- **`metadata`**: File information

#### Supported Formats

- **JSON** (`.json`): Parsed as JSON object
- **YAML** (`.yaml`, `.yml`): Parsed as YAML object
- **Text** (other extensions): Loaded as raw string

#### Example

```yaml
steps:
  - id: load_config
    component: /builtin/load_file
    input:
      path: "config/settings.yaml"

  - id: load_data
    component: /builtin/load_file
    input:
      path: { $from: { workflow: input }, path: "data_file" }
      format: "json"
```