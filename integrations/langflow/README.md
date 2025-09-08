# Stepflow Langflow Integration

A Python package for seamlessly integrating Langflow workflows with Stepflow, enabling conversion and execution of Langflow components within the Stepflow ecosystem.

## ✨ Features

- **🔄 One-Command Conversion & Execution**: Convert Langflow JSON to Stepflow YAML and execute in a single command
- **🚀 Real Langflow Execution**: Execute actual Langflow components with full type preservation
- **🔧 Tweaks System**: Modify component configurations (API keys, parameters) at execution time
- **🛡️ Security First**: Safe JSON-only parsing without requiring full Langflow installation

## 🚀 Quick Start

### Installation

```bash
cd integrations/langflow
uv sync --dev

# Or with pip
pip install -e .
```

### Basic Execution

```bash
# Execute with real Langflow components (requires OPENAI_API_KEY)
export OPENAI_API_KEY="your-api-key-here"
uv run stepflow-langflow execute tests/fixtures/langflow/basic_prompting.json '{"message": "Write a haiku about coding"}'

# Execute without environment variables using tweaks
uv run stepflow-langflow execute tests/fixtures/langflow/basic_prompting.json \
  '{"message": "Write a haiku"}' \
  --tweaks '{"LanguageModelComponent-xyz123": {"api_key": "your-api-key-here"}}'
```

### Using Tweaks for Configuration

The tweaks system allows you to modify component configurations at execution time, perfect for:
- Setting API keys without environment variables
- Adjusting model parameters 
- Debugging component configurations

```bash
# Basic tweaks example - modify API key and temperature
uv run stepflow-langflow execute my-workflow.json '{"message": "Hello"}' \
  --tweaks '{"OpenAIComponent-abc123": {"api_key": "sk-...", "temperature": 0.7}}'

# Multiple component tweaks
uv run stepflow-langflow execute my-workflow.json '{"message": "Hello"}' \
  --tweaks '{
    "LanguageModelComponent-xyz": {"api_key": "sk-..."},
    "EmbeddingComponent-def": {"api_key": "sk-...", "model": "text-embedding-3-small"}
  }'
```

### Debugging and Inspection

```bash
# Dry-run to see converted workflow without execution
uv run stepflow-langflow execute my-workflow.json --dry-run

# Apply tweaks and see the modified workflow
uv run stepflow-langflow execute my-workflow.json --dry-run \
  --tweaks '{"LanguageModelComponent-xyz": {"temperature": 0.9}}'

# Keep temporary files for debugging
uv run stepflow-langflow execute my-workflow.json '{"message": "Hello"}' \
  --keep-files --output-dir ./debug

# Analyze workflow structure before execution
uv run stepflow-langflow analyze my-workflow.json

# Convert to Stepflow YAML for inspection
uv run stepflow-langflow convert my-workflow.json output.yaml
```

### Finding Component IDs for Tweaks

To apply tweaks, you need the component IDs from your Langflow workflow:

```bash
# Method 1: Analyze the workflow to see component structure
uv run stepflow-langflow analyze my-workflow.json

# Method 2: Dry-run to see the converted Stepflow YAML with component IDs
uv run stepflow-langflow execute my-workflow.json --dry-run

# Method 3: Open the Langflow JSON file and look for node IDs
# Component IDs follow the pattern: ComponentType-nodeId (e.g., "LanguageModelComponent-kBOja")
```

## 🛠️ CLI Reference

### `execute` Command

The primary command for converting and executing Langflow workflows:

```bash
uv run stepflow-langflow execute [OPTIONS] INPUT_FILE [INPUT_JSON]
```

**Arguments:**
- `INPUT_FILE`: Path to Langflow JSON workflow file
- `INPUT_JSON`: JSON input data for the workflow (default: `{}`)

**Options:**
- `--config PATH`: Use custom stepflow configuration file
- `--stepflow-binary PATH`: Path to stepflow binary (auto-detected if not specified)
- `--timeout INTEGER`: Execution timeout in seconds (default: 60)
- `--dry-run`: Only convert, don't execute (shows converted YAML)
- `--keep-files`: Keep temporary files after execution
- `--output-dir PATH`: Directory to save temporary files

**Examples:**

```bash
# Basic execution with real Langflow components
uv run stepflow-langflow execute examples/simple_chat.json '{"message": "Hi there!"}'

# Execution with custom timeout
uv run stepflow-langflow execute examples/basic_prompting.json '{"message": "Write a haiku"}' --timeout 120

# Debug with file preservation
uv run stepflow-langflow execute examples/memory_chatbot.json --dry-run --keep-files
```

### Other Commands

```bash
# Analyze workflow dependencies and structure
uv run stepflow-langflow analyze workflow.json

# Convert with validation and pretty printing
uv run stepflow-langflow convert workflow.json --validate --pretty

# Validate workflow structure
uv run stepflow-langflow validate workflow.json

# Start component server for Stepflow integration
uv run stepflow-langflow serve
```

## 🐍 Python API

```python
from stepflow_langflow_integration.converter.translator import LangflowConverter

# Basic conversion
converter = LangflowConverter()
stepflow_yaml = converter.convert_file("workflow.json")

# Analyze workflow structure
with open("workflow.json") as f:
    langflow_data = json.load(f)

analysis = converter.analyze(langflow_data)
print(f"Nodes: {analysis['node_count']}")
print(f"Component types: {analysis['component_types']}")

# Save converted workflow
with open("output.yaml", "w") as f:
    f.write(stepflow_yaml)
```

## 🧪 Testing & Development

### Running Tests

```bash
# Run all tests
uv run pytest

# Run with coverage
uv run pytest --cov=stepflow_langflow_integration --cov-report=html

# Run integration tests with example flows
uv run pytest tests/integration/
```

### Development Setup

```bash
# Development installation with all dependencies
uv sync --dev

# Code formatting and linting
uv run ruff format
uv run ruff check --fix

# Type checking
uv run mypy src tests
```


## 📚 Example Flows

The `tests/fixtures/langflow/` directory contains example workflows from official Langflow starter projects that work with real Langflow component execution.

### Available Example Flows

All workflows accept a `"message"` field as their primary input:

| Workflow | Description | Status |
|----------|-------------|---------|
| `simple_chat.json` | Direct message passthrough | ✅ Working |
| `basic_prompting.json` | LLM with prompt template | ✅ Working |
| `memory_chatbot.json` | Conversational AI with memory | ✅ Working |
| `document_qa.json` | Document-based Q&A | ✅ Working |
| `vector_store_rag.json` | Retrieval augmented generation | ✅ Working |
| `simple_agent.json` | AI agent with tools | ⚠️ Use --timeout 180 |

### Running Example Flows

#### With Environment Variables
```bash
# Set API key in environment
export OPENAI_API_KEY="sk-your-key-here"

# Run any example flow
uv run stepflow-langflow execute tests/fixtures/langflow/basic_prompting.json \
  '{"message": "Write a haiku about coding"}'
```

#### With Tweaks (No Environment Variables)
```bash
# First, find the component ID that needs API key configuration
uv run stepflow-langflow analyze tests/fixtures/langflow/basic_prompting.json

# Then execute with tweaks
uv run stepflow-langflow execute tests/fixtures/langflow/basic_prompting.json \
  '{"message": "Write a haiku about coding"}' \
  --tweaks '{"LanguageModelComponent-kBOja": {"api_key": "sk-your-key-here"}}'
```

### Complex Example: Vector Store RAG

This example demonstrates using tweaks with multiple components:

```bash
# Analyze to find component IDs
uv run stepflow-langflow analyze tests/fixtures/langflow/vector_store_rag.json

# Execute with multiple component tweaks
uv run stepflow-langflow execute tests/fixtures/langflow/vector_store_rag.json \
  '{"message": "Explain machine learning"}' \
  --tweaks '{
    "LanguageModelComponent-abc123": {
      "api_key": "sk-your-openai-key",
      "temperature": 0.7
    },
    "EmbeddingComponent-def456": {
      "api_key": "sk-your-openai-key",
      "model": "text-embedding-3-small"
    }
  }'
```

### Workflow Input Patterns

All workflows expect JSON input with a `message` field:

| Input Example | Use Case |
|---------------|----------|
| `{"message": "Hello there!"}` | Basic greeting or question |
| `{"message": "Write a poem about nature"}` | Creative prompts for LLM |
| `{"message": "Remember I like coffee"}` | Conversational input with memory |
| `{"message": "Summarize this document"}` | Questions about documents |
| `{"message": "Calculate 42 * 17"}` | Tasks requiring tools/calculations |
| `{"message": "Explain machine learning"}` | Knowledge retrieval queries |

### Debugging Workflows

```bash
# Analyze workflow structure and find component IDs
uv run stepflow-langflow analyze tests/fixtures/langflow/vector_store_rag.json

# View converted workflow without execution
uv run stepflow-langflow execute tests/fixtures/langflow/basic_prompting.json --dry-run

# Keep temporary files for inspection
uv run stepflow-langflow execute tests/fixtures/langflow/memory_chatbot.json \
  '{"message": "Test"}' --keep-files --output-dir ./debug
```

## 🔧 Advanced Usage

### Working with Your Own Flows

```bash
# Convert your Langflow export to Stepflow
uv run stepflow-langflow convert your-workflow.json your-workflow.yaml

# Analyze your workflow structure
uv run stepflow-langflow analyze your-workflow.json

# Execute with custom configuration
uv run stepflow-langflow execute your-workflow.json \
  '{"your_input": "value"}' \
  --tweaks '{"YourComponent-id": {"api_key": "sk-..."}}'
```

### Tweaks System Reference

Tweaks modify component configurations at execution time:

```json
{
  "ComponentType-nodeId": {
    "field_name": "new_value",
    "api_key": "sk-your-key",
    "temperature": 0.8,
    "model": "gpt-4"
  }
}
```

Common tweak patterns:
- **API Keys**: `{"api_key": "sk-your-key"}`
- **Model Parameters**: `{"temperature": 0.7, "max_tokens": 1000}`
- **Model Selection**: `{"model": "gpt-4", "provider": "openai"}`
- **Custom Settings**: Any component field can be modified

### Manual Stepflow Integration

For advanced use cases with custom Stepflow configurations:

```bash
# Convert workflow
uv run stepflow-langflow convert workflow.json workflow.yaml

# Create custom stepflow-config.yml
# Run with Stepflow directly
cargo run -- run --flow workflow.yaml --input input.json --config stepflow-config.yml
```

## 🆘 Support

- **Examples**: Comprehensive examples in `tests/fixtures/langflow/`  
- **Issues**: Report bugs via [GitHub Issues](https://github.com/stepflow-ai/stepflow/issues)
- **Testing**: Use `--dry-run` mode for safe experimentation

## 📄 License

This project follows the same Apache 2.0 license as the main Stepflow project.