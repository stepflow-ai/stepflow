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
- How to structure the eval component arguments
- Basic nested workflow execution

**Files**:
- `simple-eval.yaml` - The workflow definition
- `input.json` - Empty input (not needed for this example)

**Run it**:
```bash
./target/debug/stepflow-main run \
  --flow=examples/eval/simple-eval.yaml \
  --config=examples/eval/stepflow-config.yml \
  --input=examples/eval/input.json
```

**Expected Output**:
```json
{
  "result": {
    "result": "Hello from nested workflow!",
    "execution_id": "uuid-string"
  }
}
```

### 2. Math Eval Test (`math-eval.yaml`)

**Purpose**: Demonstrates nested workflow with actual computation steps.

**What it demonstrates**:
- Nested workflow with real components (Python math functions)
- Passing data from parent workflow to nested workflow
- Multi-step nested execution
- Integration between eval and other component types

**Files**:
- `math-eval.yaml` - Workflow with nested math computation
- `math-input.json` - Input data for the workflow

**Run it**:
```bash
./target/debug/stepflow-main run \
  --flow=examples/eval/math-eval.yaml \
  --config=examples/eval/stepflow-config.yml \
  --input=examples/eval/math-input.json
```

**Expected Output**:
```json
{
  "math_result": {
    "result": {
      "sum_result": 8
    },
    "execution_id": "uuid-string"
  }
}
```

## Configuration

The `stepflow-config.yml` includes:

```yaml
plugins:
  - name: builtins
    type: builtin
  - name: python  # Only needed for math-eval example
    type: stdio
    command: uv
    args: ["--project", "../../sdks/python", "run", "stepflow_sdk"]
```

## Troubleshooting

### Common Issues

1. **"Component not found" errors**
   - Make sure the builtin plugin is registered in your config
   - Verify the component URLs (e.g., `builtins://eval`)

2. **"Expected object, got null" errors**
   - Check that your nested workflow input data is properly structured
   - Verify that referenced steps exist and produce the expected outputs

3. **Execution hangs**
   - Check for circular dependencies in your nested workflows
   - Ensure all required inputs are provided

### Debug Tips

- Use `RUST_LOG=debug` for detailed execution logs
- Start with the simple example before trying complex nested workflows
- Check that all referenced components exist and are properly registered

## Advanced Usage

Once you've verified the basic examples work, you can use eval components for:

### Regional Analysis Pattern
```yaml
- id: analyze_region
  component: "builtins://eval"
  args:
    workflow:
      steps:
        - id: filter_data
          component: python://filter_by_field
          args:
            data: { $from: "$input", path: "sales_data" }
            field: "region"
            value: { $from: "$input", path: "region_name" }
        - id: calculate_metrics
          component: python://sum_field
          args:
            data: { $from: "filter_data", path: "filtered_data" }
            field: "revenue"
      outputs:
        region_revenue: { $from: "calculate_metrics", path: "result" }
    input:
      sales_data: { $from: "load_data", path: "data" }
      region_name: "West"
```

### Dynamic Workflow Generation
```yaml
- id: process_all_regions
  component: "builtins://eval"
  args:
    workflow: { $from: "generate_region_workflow", path: "workflow_def" }
    input: { $from: "prepare_region_data", path: "data" }
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
2. Try `math-eval.yaml` to see nested computation
3. Explore the data pipeline example (`examples/data-pipeline/`) for a real-world use case
4. Build your own nested workflows for your specific use cases
