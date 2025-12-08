---
sidebar_position: 4
---

# Input and Output

Stepflow flows define clear input and output contracts using JSON Schema.
This ensures data validation, type safety, and clear documentation of what data flows into and out of your flows.

* The [Input Schema](#input-schema) describes the expected structure of input data.
* The [Output](#output) section maps step results to the flow output.
* The [Output Schema](#output-schema) defines the structure of the output data.

## Flow Input Schema {#input-schema}

Every flow should define an input schema that describes the expected structure of input data:

```yaml
inputSchema:
  type: object
  properties:
    user_id:
      type: string
      description: "Unique identifier for the user"
    api_key:
      type: string
      is_secret: true
      description: "API key for external service"
    preferences:
      type: object
      properties:
        theme:
          type: string
          enum: ["light", "dark"]
        notifications:
          type: boolean
      required: ["theme"]
    data_files:
      type: array
      items:
        type: string
      description: "List of file paths to process"
  required: ["user_id", "api_key"]
```

### Secret Fields {#secret-fields}

Mark sensitive fields as secrets using `is_secret: true` to ensure they are redacted in logs and error messages:

```yaml
inputSchema:
  type: object
  properties:
    database_url:
      type: string
      is_secret: true
      description: "Database connection string"
    config:
      type: object
      properties:
        timeout:
          type: integer
        auth_token:
          type: string
          is_secret: true
          description: "Authentication token"
      required: ["auth_token"]
```

**Secret Redaction Benefits:**
- Prevents accidental exposure in logs
- Protects sensitive data in error messages
- Maintains security in debug output
- Works automatically across all Stepflow components

For more details on secret configuration and best practices, see [Secrets](./secrets.md).

## Flow Output {#output}

Map workflow output to step results:

```yaml
output:
  summary:
    total_processed: { $from: { step: count_items } }
    success_count: { $from: { step: analyze_results }, path: "success_count" }
    error_count: { $from: { step: analyze_results }, path: "error_count" }
  results: { $from: { step: collect_results } }
```

The output section uses [Expressions](./expressions.md) to reference data from steps within the flow.

### Flow Output Schema {#output-schema}

Define the structure of data your workflow produces:

```yaml
outputSchema:
  type: object
  properties:
    summary:
      type: object
      properties:
        total_processed:
          type: integer
        success_count:
          type: integer
        error_count:
          type: integer
      required: ["total_processed", "success_count", "error_count"]
    results:
      type: array
      items:
        type: object
        properties:
          id:
            type: string
          status:
            type: string
            enum: ["success", "error", "skipped"]
          data:
            type: object
        required: ["id", "status"]
  required: ["summary", "results"]
```