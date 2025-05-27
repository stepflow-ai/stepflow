# Sales Data Pipeline Example

This example demonstrates a complete data processing pipeline that:

1. **Processes sales data** using Python components
2. **Calculates multiple metrics** in parallel
3. **Shows data flow dependencies** between steps
4. **Demonstrates concurrent execution**

## What This Shows

- **Parallel Processing**: Revenue sum, count, and average calculations run simultaneously
- **Data Dependencies**: Performance ratio waits for total revenue calculation
- **Multi-Component Workflow**: Combines multiple Python components in a pipeline
- **Real-World Use Case**: Practical business data analysis

## Running the Example

```bash
# From the project root
cargo build

# Run the pipeline
./target/debug/stepflow-main run \
  --flow=examples/data-pipeline/pipeline.yaml \
  --input=examples/data-pipeline/sales-data.json

# Or pipe the input
cat examples/data-pipeline/sales-data.json | \
  ./target/debug/stepflow-main run \
  --flow=examples/data-pipeline/pipeline.yaml
```

## Expected Output

```json
{
  "total_revenue": 10180.0,
  "sales_count": 8,
  "average_sale": 1272.5,
  "performance_ratio": 127.25
}
```

## How It Works

1. **Input**: Sales records with revenue data and a target revenue goal
2. **Parallel Processing**: Three calculations run simultaneously:
   - Sum all revenue values
   - Count total sales
   - Calculate average sale amount
3. **Dependent Step**: Performance ratio calculation waits for total revenue
4. **Output**: Structured results showing all calculated metrics

## The Data Flow

```
sales_data ──┬── sum_field ──────┐
             ├── count_items      │
             └── average_field    │
                                  │
target_revenue ──────────────────┴── divide ──► performance_ratio
```

This demonstrates how StepFlow automatically handles dependencies and runs independent steps in parallel.
