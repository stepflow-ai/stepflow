# Stepflow Langflow Integration

A Python package for seamlessly integrating Langflow workflows with Stepflow, enabling conversion and execution of Langflow components within the Stepflow ecosystem.

## Features

- **JSON-First Conversion**: Fast, secure conversion of Langflow JSON workflows to Stepflow YAML
- **100% Component Compatibility**: Execute original Langflow components via UDF execution
- **Native Type Support**: Preserve Langflow `Message`, `Data`, and `DataFrame` semantics
- **CLI Tools**: Easy-to-use command-line interface for conversion and execution
- **Lightweight**: Minimal dependencies, no full Langflow installation required

## Quick Start

### Installation

```bash
cd integrations/langflow
pip install -e .

# Or with uv
uv pip install -e .
```

### Convert a Langflow Workflow

```bash
# Convert Langflow JSON to Stepflow YAML
stepflow-langflow convert my-workflow.json output.yaml

# Convert with validation
stepflow-langflow convert my-workflow.json output.yaml --validate

# Analyze workflow structure
stepflow-langflow analyze my-workflow.json
```

### Use with Stepflow

1. **Configure Stepflow** (`stepflow-config.yml`):
```yaml
plugins:
  langflow:
    type: stepflow
    transport: stdio
    command: stepflow-langflow
    args: ["serve"]

routes:
  "/langflow/{*component}":
    - plugin: langflow
```

2. **Run your converted workflow**:
```bash
cargo run -- run --flow output.yaml --input input.json --config stepflow-config.yml
```

## Python API

```python
from stepflow_langflow_integration import LangflowConverter

# Convert workflow
converter = LangflowConverter()
stepflow_yaml = converter.convert_file("workflow.json")

# Convert with validation
converter = LangflowConverter(validate_schemas=True)
stepflow_yaml = converter.convert_file("workflow.json")

# Save as YAML
with open("output.yaml", "w") as f:
    f.write(stepflow_yaml)
```

## How It Works

1. **JSON Parsing**: Directly parses Langflow JSON without requiring Langflow installation
2. **Dependency Analysis**: Maps Langflow edges to Stepflow step dependencies  
3. **Schema Discovery**: Extracts component schemas from JSON metadata
4. **UDF Execution**: Runs original Langflow component code for 100% compatibility
5. **Type Preservation**: Maintains native Langflow types (`Message`, `Data`, `DataFrame`)

## Supported Components

All standard Langflow components are supported, including:

- **Chat Components**: ChatInput, ChatOutput, LanguageModel
- **Data Processing**: TextSplitter, DocumentLoader, VectorStore
- **AI Models**: OpenAI, Anthropic, Google models
- **Custom Components**: User-defined components via UDF execution

## Development

See [PLAN.md](./PLAN.md) for detailed implementation plan and development guide.

```bash
# Development setup
uv sync --dev

# Run tests
uv run pytest

# Format code
uv run ruff format

# Type check
uv run mypy src tests
```

## Architecture

Built on proven foundations:
- **JSON-First Parsing**: Based on working prototype from `fraz/translation-layer`
- **Stepflow Integration**: Uses `stepflow-py` SDK for component execution
- **Security**: No code execution during parsing, safe UDF execution

## License

This project follows the same Apache 2.0 license as the main Stepflow project.

## Support

- **Documentation**: See [PLAN.md](./PLAN.md) for comprehensive details
- **Issues**: Report bugs via GitHub Issues
- **Examples**: Check the `examples/` directory for sample workflows