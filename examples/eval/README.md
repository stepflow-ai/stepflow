# Eval Component Examples

This directory demonstrates the `builtin://eval` component, which enables **nested workflow execution** within StepFlow. The eval component allows you to define complete workflows as steps, enabling powerful composition and reusability patterns.

## What is the Eval Component?

The `builtin://eval` component executes a nested workflow with its own steps, inputs, and outputs. This enables:

- **Workflow Composition**: Build complex workflows from simpler, reusable sub-workflows
- **Isolation**: Each nested workflow runs independently with its own execution context
- **Parallel Execution**: Multiple eval components can run concurrently
- **Dynamic Workflows**: Create workflows that generate and execute other workflows

## Examples

### 1. Simple Eval Test (`simple-eval.yaml`)

**Purpose**: Basic test to verify the eval component is working correctly.

**What it demonstrates**:
- Simplest possible nested workflow (no steps, just outputs)
- How to structure the eval component input
- Basic nested workflow execution

**Files**:
- `simple-eval.yaml` - The workflow definition
- `input.json` - Empty input (not needed for this example)

**Run it**:
```bash
cargo run -- run \
  --flow=examples/eval/simple-eval.yaml \
  --config=examples/eval/stepflow-config.yml \
  --input=examples/eval/input.json
```

**Expected Output**:
```json
{
  "outcome": "success",
  "result": {
    "result": {
      "test_result": "Hello from nested workflow!"
    }
  }
}
```

### 2. Test Builtins (`test-builtins.yaml`)

**Purpose**: Tests the built-in component registry, specifically the `create_messages` component.

**What it demonstrates**:
- Using the `builtin://create_messages` component
- Proper input/output structure for builtin components

**Run it**:
```bash
cargo run -- run \
  --flow=examples/eval/test-builtins.yaml \
  --config=examples/eval/stepflow-config.yml \
  --input=examples/eval/input.json
```

**Expected Output**:
```json
{
  "outcome": "success",
  "result": {
    "messages": [
      {
        "role": "system",
        "content": "You are a test assistant"
      },
      {
        "role": "user",
        "content": "Say hello"
      }
    ]
  }
}
```

### 3. Math Eval Test (`math-eval.yaml`)

**Purpose**: Demonstrates nested workflow with actual computation steps.

**What it demonstrates**:
- Nested workflow with real components (Python math functions)
- Passing data from parent workflow to nested workflow
- Multi-step nested execution
- Integration between eval and other component types

**Prerequisites**:
- Requires Python SDK to be installed
- Requires `uv` package manager in PATH

**Files**:
- `math-eval.yaml` - Workflow with nested math computation
- `math-input.json` - Input data for the workflow

**Run it**:
```bash
cargo run -- run \
  --flow=examples/eval/math-eval.yaml \
  --config=examples/eval/stepflow-config.yml \
  --input=examples/eval/math-input.json
```

**Expected Output**:
```json
{
  "outcome": "success",
  "result": {
    "math_result": {
      "result": {
        "sum_result": 8
      },
      "execution_id": "uuid-string"
    }
  }
}
```

## Configuration

The `stepflow-config.yml` includes:

```yaml
plugins:
  - name: builtin
    type: builtin
  - name: python  # Only needed for math-eval example
    type: stdio
    command: uv
    args: ["--project", "../../sdks/python", "run", "stepflow_sdk"]
```

## Important Notes

### Syntax Changes
The examples have been updated to use the correct workflow syntax:
- Use `input` instead of `args` for step inputs
- Use `output` (singular) instead of `outputs` (plural)
- Use proper reference format: `{ $from: { step: step_id }, path: "field" }`
- Nested workflows must be wrapped in `$literal` when defined inline

### Example of Correct Syntax:
```yaml
steps:
  - id: my_eval_step
    component: "builtin://eval"
    input:  # Not 'args'
      workflow:
        $literal:  # Required for inline workflow definitions
          name: "My Nested Workflow"
          steps: []
          output:  # Not 'outputs'
            result: "Hello"
      input: {}

output:  # Not 'outputs'
  result: { $from: { step: my_eval_step }, path: "result" }
```

## Troubleshooting

### Common Issues

1. **"Component not found" errors**
   - Make sure the builtin plugin is registered in your config
   - Verify the component URLs (e.g., `builtin://eval`)

2. **"Nested flow execution was cancelled" errors**
   - Check that output references use the correct format
   - Ensure the workflow waits for step completion before resolving output

3. **Steps stuck "Waiting for inputs"**
   - Verify you're using `input:` not `args:` for step inputs
   - Check that all required input fields are provided

### Debug Tips

- Use `RUST_LOG=debug` for detailed execution logs
- Start with the simple example before trying complex nested workflows
- Check that all referenced components exist and are properly registered

## Advanced Usage

Once you've verified the basic examples work, you can use eval components for:

### Regional Analysis Pattern
```yaml
- id: analyze_region
  component: "builtin://eval"
  input:
    workflow:
      $literal:
        steps:
          - id: filter_data
            component: python://filter_by_field
            input:
              data: { $from: { workflow: input }, path: "sales_data" }
              field: "region"
              value: { $from: { workflow: input }, path: "region_name" }
          - id: calculate_metrics
            component: python://sum_field
            input:
              data: { $from: { step: filter_data }, path: "filtered_data" }
              field: "revenue"
        output:
          region_revenue: { $from: { step: calculate_metrics }, path: "result" }
    input:
      sales_data: { $from: { step: load_data }, path: "data" }
      region_name: "West"
```

### Dynamic Workflow Generation
```yaml
- id: process_all_regions
  component: "builtin://eval"
  input:
    workflow: { $from: { step: generate_region_workflow }, path: "workflow_def" }
    input: { $from: { step: prepare_region_data }, path: "data" }
```

## Real-World Applications

The eval component is particularly useful for:

- **Data Processing Pipelines**: Apply the same analysis to different data subsets
- **A/B Testing**: Run different workflow variants and compare results
- **Multi-tenant Processing**: Execute customer-specific workflows
- **Dynamic Analysis**: Generate workflows based on data characteristics
- **Fault Isolation**: Isolate potentially failing workflows from main execution

## Next Steps

1. Start with `simple-eval.yaml` to verify basic functionality
2. Try `test-builtins.yaml` to test other builtin components
3. Try `math-eval.yaml` if you have the Python SDK set up
4. Explore the data pipeline example (`examples/data-pipeline/`) for a real-world use case
5. Build your own nested workflows for your specific use cases