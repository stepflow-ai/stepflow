# Basic Math Operations Example

This example demonstrates basic mathematical operations using **User-Defined Functions (UDFs)** instead of pre-built components. This approach shows how simple operations can be defined inline as code rather than requiring specialized components.

## What This Example Shows

- **UDF Creation**: How to create and store Python code as blobs for reuse
- **Basic Math Operations**: Addition and multiplication using UDFs
- **Workflow Composition**: Chaining operations where one step uses the result of another
- **Testing**: Embedded test cases to verify the workflow works correctly

## Requirements

The example requires the `uv` package manager to be installed. This is most easily accomplished via `brew install uv` on macOS.

## Running the Example

```sh
# From the project root directory
cd stepflow-rs
cargo build

# Run with file input
cargo run -- run --flow=../examples/basic/workflow.yaml --input=../examples/basic/input1.json --config=../examples/basic/stepflow-config.yml

# Run with inline input
cargo run -- run --flow=../examples/basic/workflow.yaml --input-json='{"m": 3, "n": 4}' --config=../examples/basic/stepflow-config.yml

# Run with piped input
echo '{"m": 5, "n": 12}' | cargo run -- run --flow=../examples/basic/workflow.yaml --config=../examples/basic/stepflow-config.yml

# Run the embedded tests
cargo run -- test ../examples/basic/workflow.yaml --config=../examples/basic/stepflow-config.yml
```

## Expected Output

With input `{"m": 3, "n": 4}`:
```json
{
  "m_plus_n": 7,
  "m_times_n": 12,
  "m_plus_n_times_n": 28
}
```

The workflow calculates:
- `m_plus_n`: 3 + 4 = 7
- `m_times_n`: 3 * 4 = 12
- `m_plus_n_times_n`: (3 + 4) * 4 = 28