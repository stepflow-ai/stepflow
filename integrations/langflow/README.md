# Stepflow Langflow Integration

A Python package for seamlessly integrating Langflow workflows with Stepflow, enabling conversion and execution of Langflow components within the Stepflow ecosystem.

## ✨ Features

- **🔄 One-Command Conversion & Execution**: Convert Langflow JSON to Stepflow YAML and execute in a single command
- **🚀 Real Langflow Execution**: Execute actual Langflow components with full type preservation
- **🔧 Runtime Tweaks System**: Modify component configurations (API keys, parameters) at execution time using StepFlow's native override system
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

### Using Runtime Tweaks for Configuration

The runtime tweaks system allows you to modify component configurations at execution time using StepFlow's native override mechanism, perfect for:
- Setting API keys without environment variables
- Adjusting model parameters dynamically
- Debugging component configurations
- A/B testing different parameter values

Runtime tweaks are converted to StepFlow WorkflowOverrides format and applied during workflow execution, rather than modifying the original workflow definition.

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

### `submit-batch` Command

Execute a Langflow workflow with multiple inputs using batch processing. This command converts the workflow once and then processes multiple inputs efficiently.

```bash
uv run stepflow-langflow submit-batch [OPTIONS] INPUT_FILE INPUTS_JSONL
```

**Arguments:**
- `INPUT_FILE`: Path to Langflow JSON workflow file
- `INPUTS_JSONL`: Path to JSONL file with batch inputs (one JSON object per line)

**Options:**
- `--url TEXT`: Stepflow server URL (required, e.g., `http://localhost:7837/api/v1`)
- `--tweaks TEXT`: JSON tweaks to apply to the workflow (same format as `execute` command)
- `--max-concurrent INTEGER`: Maximum number of concurrent executions (optional)
- `--output PATH`: Path to output JSONL file for results (optional)

**JSONL Input Format:**

Each line in the inputs JSONL file should be a complete JSON object representing one execution input:

```jsonl
{"message": "Write a haiku about coding"}
{"message": "Explain quantum computing"}
{"message": "What is machine learning?"}
{"message": "Tell me a joke"}
{"message": "Summarize photosynthesis"}
```

**Output Format:**

When `--output` is specified, results are written as JSONL with the following structure:

```jsonl
{"index": 0, "status": "completed", "result": {"outcome": "success", "result": {...}}}
{"index": 1, "status": "failed", "result": {"outcome": "failure", "error": {...}}}
{"index": 2, "status": "completed", "result": {"outcome": "success", "result": {...}}}
```

Each result includes:
- `index`: Input index (0-based)
- `status`: Either "completed" or "failed"
- `result`: Workflow execution result or error details

**Examples:**

```bash
# Basic batch execution with running server
uv run stepflow-langflow submit-batch \
  examples/basic_prompting.json \
  inputs.jsonl \
  --url http://localhost:7837/api/v1

# Batch execution with tweaks (API keys, parameters)
uv run stepflow-langflow submit-batch \
  examples/basic_prompting.json \
  inputs.jsonl \
  --url http://localhost:7837/api/v1 \
  --tweaks '{"LanguageModelComponent-kBOja": {"api_key": "sk-...", "temperature": 0.7}}'

# Batch execution with concurrency limit and output file
uv run stepflow-langflow submit-batch \
  examples/vector_store_rag.json \
  queries.jsonl \
  --url http://localhost:7837/api/v1 \
  --max-concurrent 5 \
  --output results.jsonl

# Complete workflow: start server, run batch, analyze results
# Terminal 1: Start Stepflow server
stepflow-server --port 7837 --config stepflow-config.yml

# Terminal 2: Submit batch job
uv run stepflow-langflow submit-batch \
  examples/basic_prompting.json \
  batch_inputs.jsonl \
  --url http://localhost:7837/api/v1 \
  --output batch_results.jsonl

# Analyze results
cat batch_results.jsonl | jq '.result.result'
```

**Exit Codes:**
- Returns 0 if all executions completed successfully
- Returns non-zero if any executions failed (results still written to output file)

**Performance Characteristics:**
- Workflow conversion happens once (not per input)
- Tweaks are applied once (not per input)
- Inputs are processed concurrently by the Stepflow server
- Use `--max-concurrent` to control resource usage

### `tweak` Command

Convert Langflow tweaks to StepFlow runtime overrides format. This is useful for understanding how tweaks map to StepFlow overrides or for generating override files.

```bash
uv run stepflow-langflow tweak [OPTIONS] STEPFLOW_FILE [OUTPUT_FILE]
```

**Arguments:**
- `STEPFLOW_FILE`: Path to Stepflow YAML workflow file
- `OUTPUT_FILE`: Path for output file (optional, prints to stdout if not provided)

**Options:**
- `--tweaks TEXT`: JSON tweaks to convert (required)

**Examples:**

```bash
# Convert tweaks to overrides format (output to stdout)
uv run stepflow-langflow tweak workflow.yaml \
  --tweaks '{"LanguageModelComponent-kBOja": {"api_key": "new_key", "temperature": 0.8}}'

# Output: StepFlow WorkflowOverrides YAML
# stepOverrides:
#   langflow_LanguageModelComponent-kBOja:
#     input.api_key: new_key
#     input.temperature: 0.8

# Save overrides to file for use with stepflow CLI
uv run stepflow-langflow tweak workflow.yaml overrides.yaml \
  --tweaks '{"Component-123": {"param": "value"}}'

# Use the generated overrides with stepflow directly
stepflow run --flow workflow.yaml --input input.json --overrides overrides.yaml
```

**Use Cases:**
- **Understanding mappings**: See how Langflow tweaks translate to StepFlow overrides
- **Manual override generation**: Create override files for direct use with StepFlow CLI
- **Integration debugging**: Inspect the exact override format being generated
- **Workflow development**: Generate overrides for testing different parameter combinations

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

### Runtime Overrides System

Langflow tweaks are now implemented using StepFlow's native runtime override system, providing several key benefits:

**How It Works:**
1. Langflow tweaks are converted to StepFlow `WorkflowOverrides` format
2. Overrides are applied during workflow execution, not during conversion
3. Original workflow remains unchanged
4. Compatible with StepFlow's override CLI options

**Tweaks Input Format (Langflow style):**
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

**Generated Override Format (StepFlow style):**
```yaml
stepOverrides:
  langflow_ComponentType-nodeId:
    input.field_name: new_value
    input.api_key: sk-your-key
    input.temperature: 0.8
    input.model: gpt-4
```

**Benefits of Runtime Overrides:**
- **Efficiency**: No workflow modification needed - faster execution
- **Server Support**: Compatible with StepFlow server submission (when server supports overrides)
- **Clean Separation**: Original workflows remain unchanged for debugging
- **Native Integration**: Uses StepFlow's built-in override mechanism
- **Consistency**: Same override system used across all StepFlow integrations

**Common Tweak Patterns:**
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