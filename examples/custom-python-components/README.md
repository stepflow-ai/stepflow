# Custom Python Components Example

This example demonstrates how to use the Stepflow Python SDK as a **library** to create custom component servers with domain-specific business logic.

## What This Example Shows

- **Custom Component Server**: How to create a standalone Python server using the Stepflow SDK
- **Business Logic Components**: Domain-specific components for customer analysis
- **Typed Interfaces**: Using `msgspec` for type-safe component inputs and outputs
- **Async Context Usage**: Using `StepflowContext` for blob operations
- **Real Business Use Case**: Customer analytics and report generation

## Architecture

This example includes:

1. **`custom_server.py`**: A custom Python component server built with the Stepflow SDK
2. **`workflow.yaml`**: A workflow that uses the custom components
3. **Business Logic**: Customer analysis, report generation, and recommendations

## Custom vs UDF Approach

This example demonstrates when to use **custom components** instead of **UDFs**:

- ✅ **Custom Components** (this example):
  - Complex business logic with multiple functions
  - Typed domain objects (Customer, Order, etc.)
  - Async operations with context (blob storage)
  - Reusable across multiple workflows
  - Team wants to maintain business logic separately

- ✅ **UDFs** (see basic example):
  - Simple mathematical operations
  - One-off transformations
  - Inline code that's workflow-specific

## Requirements

1. **Python 3.8+** with the dependencies listed in `requirements.txt`
2. **Stepflow Rust CLI** (built from the project root)

## Setup

```sh
# Install Python dependencies (optional - uses system Python)
pip install -r requirements.txt

# Make the custom server executable
chmod +x custom_server.py
```

## Running the Example

```sh
# From the project root directory
cd stepflow-rs
cargo build

# Run with file input
cargo run -- run --flow=../examples/custom-python-components/workflow.yaml --input=../examples/custom-python-components/input.json --config=../examples/custom-python-components/stepflow-config.yml

# Run the embedded tests
cargo run -- test ../examples/custom-python-components/workflow.yaml --config=../examples/custom-python-components/stepflow-config.yml
```

## Expected Output

The workflow will:

1. **Analyze customer data** using the `/custom/analyze_customers` component
2. **Generate a business report** using the `/custom/generate_report` component
3. **Store the report as a blob** and return a summary

Sample output structure:
```json
{
  "analysis": {
    "total_revenue": 5755.0,
    "tier_breakdown": {
      "bronze": 235.0,
      "silver": 770.0,
      "gold": 4750.0
    },
    "top_customers": [
      {"id": "cust_004", "name": "David Wilson", "tier": "gold"},
      {"id": "cust_001", "name": "Alice Johnson", "tier": "gold"},
      {"id": "cust_002", "name": "Bob Smith", "tier": "silver"}
    ]
  },
  "report_summary": "Generated Q4 Customer Analysis Report with $5,755.00 total revenue. Top customers: David Wilson, Alice Johnson, Bob Smith",
  "detailed_report": {
    "title": "Q4 Customer Analysis Report",
    "customer_analysis": { ... },
    "recommendations": [
      "Focus on customer acquisition to diversify revenue"
    ]
  }
}
```

## How It Works

### 1. Custom Server Setup

The `custom_server.py` file demonstrates the Python SDK library usage:

```python
from stepflow_server import StepflowStdioServer, StepflowContext

# Create server instance
server = StepflowStdioServer()

# Register components with decorator
@server.component
def analyze_customers(input: CustomerAnalysisInput) -> CustomerAnalysisOutput:
    # Business logic here
    return result

# Run the server
server.run()
```

### 2. Component Registration

Components are registered using the `@server.component` decorator, which:
- Automatically extracts input/output schemas from type hints
- Handles JSON serialization/deserialization
- Supports both sync and async functions
- Optionally receives `StepflowContext` for blob operations

### 3. Workflow Integration

The workflow uses the custom components via the `/custom/` path:

```yaml
steps:
  - id: analyze_customer_data
    component: "/custom/analyze_customers"
    input:
      customers: { $input: "customers" }
      orders: { $input: "orders" }
```

## Benefits of This Approach

1. **Type Safety**: Full type checking for inputs and outputs
2. **Reusability**: Components can be used across multiple workflows
3. **Maintainability**: Business logic is organized and testable
4. **Performance**: Persistent server avoids startup costs
5. **Integration**: Seamless blob storage and context operations

This pattern is ideal for teams that want to build reusable business logic components while leveraging Stepflow's workflow orchestration capabilities.