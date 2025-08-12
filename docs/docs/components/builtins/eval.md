---
sidebar_position: 4
---

### `eval`

Execute nested workflows with isolated execution contexts.

#### Input

```yaml
input:
  workflow:
    inputs: { }
    steps: [ ]
    output: { }
  input: <data for nested workflow>
```

- **`workflow`** (required): Complete workflow definition to execute
- **`input`** (required): Input data for the nested workflow

#### Output

```yaml
output:
  result: <output from nested workflow>
  run_id: "uuid-of-execution"
```

- **`result`**: The output produced by the nested workflow
- **`run_id`**: Unique identifier for the nested execution

#### Example

```yaml
steps:
  - id: run_analysis
    component: /builtin/eval
    input:
      workflow:
        input_schema:
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
      input:
        data: { $from: { step: load_data } }
```

#### Use Cases

- **Workflow Modularity**: Break complex workflows into reusable sub-workflows
- **Dynamic Execution**: Execute different workflows based on runtime conditions
- **Parallel Sub-workflows**: Run multiple independent workflows concurrently
- **Testing**: Execute workflows in isolation for testing purposes