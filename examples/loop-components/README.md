# Loop Components Example

This example demonstrates looping functionality in StepFlow, including both built-in components and Python implementations that use the new `evaluate_flow` method.

## Components

### Built-in Components
- **`iterate`**: Iteratively applies a workflow until it returns `{"result": value}` instead of `{"next": value}`
- **`map`**: Applies a workflow to each item in a list and collects the results

### Python Components (via `loop_server.py`)
- **`/python/iterate`**: Python implementation of iterate using `context.evaluate_flow()`. If any iteration is skipped or fails, the whole iterate component will be skipped or failed.
- **`/python/map`**: Python implementation of map using `context.evaluate_flow()`. If any item is skipped or fails, the whole map component will be skipped or failed.

### Helper Components
- **`counter_step`**: Counts towards a target, demonstrating the iterate pattern
- **`double_value`**: Doubles a value, demonstrating the map pattern

## Example Files

Each example includes comprehensive test cases that validate the functionality:

### `iterate.yaml`
Demonstrates Python iterate component with counter workflow
- Tests various counting scenarios (0→5, 3→7, already at target)
- Includes input/output schemas and test assertions

### `map_example.yaml`
Demonstrates Python map component with doubling workflow
- Tests array processing (small numbers, mixed numbers, empty arrays)
- Includes comprehensive result validation

### `builtin_iterate.yaml`
Tests the built-in iterate component with Python helper components
- Validates built-in iterate works with external component services
- Tests edge cases like already being at target

## Usage

### Run the iterate example:
```bash
cargo run -- run --flow=examples/loop-components/iterate.yaml --input=examples/loop-components/input.json --config=examples/loop-components/stepflow-config.yml
```

### Run the map example:
```bash
cargo run -- run --flow=examples/loop-components/map_example.yaml --input=examples/loop-components/input.json --config=examples/loop-components/stepflow-config.yml
```

### Run the builtin iterate test:
```bash
cargo run -- run --flow=examples/loop-components/builtin_iterate.yaml --input=examples/loop-components/input.json --config=examples/loop-components/stepflow-config.yml
```

### Run tests for validation:
```bash
cargo insta test --unreferenced=delete --review
```

## How It Works

1. **Protocol Extension**: Added `flows/evaluate` method to the bidirectional protocol, allowing components to request workflow execution from the runtime.

2. **Built-in Implementation**: The `iterate` and `map` components are implemented in Rust as built-in components with proper error handling and safety limits.

3. **Python Implementation**: The Python components demonstrate how external services can implement complex control flow using the `evaluate_flow()` method. The simplified exception-based approach means that any skip/failure in individual iterations or map items will cause the entire operation to skip/fail.

4. **Bidirectional Communication**: Components can send workflows back to the runtime for execution, enabling sophisticated control flow patterns.

5. **Test Coverage**: Each example includes comprehensive test cases that verify functionality across different scenarios and edge cases.

This showcases the power of the bidirectional protocol for implementing sophisticated control flow in external components while maintaining the ability to test and validate the behavior.