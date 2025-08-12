---
sidebar_position: 4
---

### `eval`

Execute nested workflows with isolated execution contexts.

#### Input

```yaml
input:
  flow_id: "sha256:abc123..."  # Blob ID of stored workflow
  input: <data for nested workflow>
```

- **`flow_id`** (required): Blob ID of the workflow to execute (must be stored as a blob first)
- **`input`** (required): Input data for the nested workflow

#### Output

```yaml
output:
  result: <output from nested workflow>
  run_id: "uuid-of-execution"
```

- **`result`**: The result from executing the nested workflow (FlowResult type)
- **`run_id`**: Unique identifier for the nested execution

#### Example

```yaml
steps:
  - id: store_workflow
    component: /builtin/put_blob
    input:
      data:
        $literal:
          schema: https://stepflow.org/schemas/v1/flow.json
          input:
            type: object
            properties:
              data: { type: array }
          steps:
            - id: process
              component: /data/analyze
              input:
                data: { $from: { workflow: input }, path: "data" }
            - id: summarize
              component: /data/summarize
              input:
                analysis: { $from: { step: process } }
          output:
            summary: { $from: { step: summarize } }
      blob_type: "flow"

  - id: run_analysis
    component: /builtin/eval
    input:
      flow_id: { $from: { step: store_workflow }, path: "blob_id" }
      input:
        data: { $from: { step: load_data } }
```

#### Use Cases

- **Workflow Modularity**: Break complex workflows into reusable sub-workflows
- **Dynamic Execution**: Execute different workflows based on runtime conditions
- **Parallel Sub-workflows**: Run multiple independent workflows concurrently
- **Testing**: Execute workflows in isolation for testing purposes