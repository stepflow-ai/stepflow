---
sidebar_position: 1
---

# Examples Overview

This section provides comprehensive examples of StepFlow workflows, from simple operations to complex AI-powered applications. Each example includes complete workflow definitions, explanations, and best practices.

## Example Categories

### 🚀 **Getting Started Examples**
- **[Basic Operations](./basic-operations.md)** - Simple workflows demonstrating core concepts
- **[AI Workflows](./ai-workflows.md)** - OpenAI integration, prompt engineering, and AI-powered analysis
- **[Data Processing](./data-processing.md)** - File handling, transformation, and validation patterns
- **[Custom Components](./custom-components.md)** - Building reusable components in Python and other languages

## How to Use These Examples

### Running Examples

All examples can be run from the StepFlow repository:

```bash
# Clone the repository
git clone https://github.com/riptano/stepflow.git
cd stepflow/stepflow-rs

# Run a basic example
cargo run -- run --flow=examples/basic/workflow.yaml --input=examples/basic/input1.json

# Run with test cases
cargo run -- test examples/basic/workflow.yaml
```

### Example Structure

Each example includes:

- **📄 Workflow Definition**: Complete YAML workflow file
- **📝 Step-by-Step Explanation**: How each part works
- **🧪 Test Cases**: Sample inputs and expected outputs
- **⚙️ Configuration**: Required plugins and settings
- **💡 Variations**: Alternative approaches and extensions

Ready to dive in? Start with [Basic Operations](./basic-operations.md) to learn the fundamentals, or jump to a specific example that interests you!