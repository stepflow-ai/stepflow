---
sidebar_position: 8
---

### `iterate`

Iteratively apply a workflow until a termination condition is met. The workflow must return either `{"result": value}` to terminate or `{"next": value}` to continue.

#### Input

```yaml
input:
  flow:
    steps: [ ]
    output: { }
  initial_input: <data for first iteration>
  max_iterations: 1000  # optional, prevents infinite loops
```

- **`flow`** (required): Complete workflow definition to execute iteratively
- **`initial_input`** (required): Input data for the first iteration
- **`max_iterations`** (optional): Maximum iterations allowed (default: 1000)

#### Output

```yaml
output:
  result: <final result value>
  iterations: 5
  terminated: false
```

- **`result`**: The final result value when workflow returns `{"result": value}`
- **`iterations`**: Number of iterations performed
- **`terminated`**: Whether iteration was stopped by `max_iterations` limit

#### Workflow Contract

The workflow being iterated must return an object with either:
- **`result`**: Final value to return (terminates iteration)
- **`next`**: Value to pass as input to next iteration (continues)

#### Example

```yaml
steps:
  - id: countdown
    component: /builtin/iterate
    input:
      flow:
        schema:
          type: object
          properties:
            count: { type: number }
        steps:
          - id: check_count
            component: /builtin/eval
            input:
              flow_id: { $from: { step: countdown_logic } }
              input: { $from: { workflow: input } }
        output:
          # Return "result" if count reaches 0, otherwise "next" with decremented count
          $if:
            condition: { $from: { workflow: input }, path: "count" }
            equals: 0
          then:
            result: "Countdown complete!"
          else:
            next:
              count: { $expr: "{{ input.count - 1 }}" }
      initial_input:
        count: 5
      max_iterations: 10
```

#### Use Cases

- **Iterative Algorithms**: Implement loops and recursive-style processing
- **Convergence Testing**: Run until results meet specific criteria
- **State Machines**: Process state transitions until final state reached
- **Retry Logic**: Keep trying an operation until success or max attempts