---
sidebar_position: 2
---

# Input and Output

StepFlow workflows define clear input and output contracts using JSON Schema. This ensures data validation, type safety, and clear documentation of what data flows into and out of your workflows.

## Workflow Input

### Input Schema Definition

Every workflow should define an input schema that describes the expected structure of input data:

```yaml
input_schema:
  type: object
  properties:
    user_id:
      type: string
      description: "Unique identifier for the user"
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
  required: ["user_id"]
```

### Input Validation

StepFlow validates input data against the schema before workflow execution:

- **Type checking**: Ensures data types match schema definitions
- **Required fields**: Validates that all required fields are present
- **Constraints**: Enforces enums, patterns, and other constraints
- **Clear errors**: Provides detailed error messages for validation failures

## Workflow Output

### Output Schema Definition

Define the structure of data your workflow produces:

```yaml
output_schema:
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

### Output Definition

Map workflow output to step results:

```yaml
output:
  summary:
    total_processed: { $from: { step: count_items } }
    success_count: { $from: { step: analyze_results }, path: "success_count" }
    error_count: { $from: { step: analyze_results }, path: "error_count" }
  results: { $from: { step: collect_results } }
```