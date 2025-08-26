# Stepflow Langflow Integration

A Python package for seamlessly integrating Langflow workflows with Stepflow, enabling conversion and execution of Langflow components within the Stepflow ecosystem.

## ✨ Features

- **🔄 One-Command Conversion & Execution**: Convert Langflow JSON to Stepflow YAML and execute in a single command
- **🎭 Dual Execution Modes**: Support both mock (testing) and real UDF execution
- **🧬 100% Component Compatibility**: Execute original Langflow components with full type preservation
- **🛡️ Security First**: Safe JSON-only parsing without requiring full Langflow installation

## 🚀 Quick Start

### Installation

```bash
cd integrations/langflow
uv sync --dev

# Or with pip
pip install -e .
```

### Convert and Execute in One Command

```bash
# Execute with mock components (safe for testing)
uv run stepflow-langflow execute my-workflow.json '{"message": "Hello!"}' --mock

# Execute with real Langflow UDF components
uv run stepflow-langflow execute my-workflow.json '{"message": "Hello!"}'

# Dry-run to see converted workflow
uv run stepflow-langflow execute my-workflow.json --dry-run

# Keep files for debugging
uv run stepflow-langflow execute my-workflow.json '{}' --mock --keep-files --output-dir ./debug
```

### Individual Commands

```bash
# Analyze workflow structure
uv run stepflow-langflow analyze my-workflow.json

# Convert Langflow JSON to Stepflow YAML
uv run stepflow-langflow convert my-workflow.json output.yaml --pretty

# Validate a Langflow workflow
uv run stepflow-langflow validate my-workflow.json

# Start the component server
uv run stepflow-langflow serve
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
- `--mock`: Use mock components instead of real UDF execution (safe for testing)
- `--config PATH`: Use custom stepflow configuration file
- `--stepflow-binary PATH`: Path to stepflow binary (auto-detected if not specified)
- `--timeout INTEGER`: Execution timeout in seconds (default: 60)
- `--dry-run`: Only convert, don't execute (shows converted YAML)
- `--keep-files`: Keep temporary files after execution
- `--output-dir PATH`: Directory to save temporary files

**Examples:**

```bash
# Basic execution with mock components
uv run stepflow-langflow execute examples/simple_chat.json '{"message": "Hi there!"}' --mock

# Real execution with custom timeout
uv run stepflow-langflow execute examples/basic_prompting.json '{"prompt": "Write a haiku"}' --timeout 120

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

## 🔧 Manual Stepflow Integration

For advanced use cases, you can integrate manually with Stepflow:

### 1. Configure Stepflow

Create `stepflow-config.yml`:

```yaml
plugins:
  builtin:
    type: builtin
  langflow:
    type: stepflow
    transport: stdio
    command: uv
    args: ["--project", "path/to/langflow/integration", "run", "stepflow-langflow-server"]

routes:
  "/langflow/{*component}":
    - plugin: langflow
  "/builtin/{*component}":
    - plugin: builtin

stateStore:
  type: inMemory
```

### 2. Convert and Run

```bash
# Convert workflow
uv run stepflow-langflow convert my-workflow.json my-workflow.yaml

# Run with Stepflow
cargo run -- run --flow my-workflow.yaml --input input.json --config stepflow-config.yml
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

## 🏗️ How It Works

The integration follows a multi-stage approach for maximum compatibility and safety:

### 1. **JSON-First Parsing**
- Directly parses Langflow JSON export files
- No Langflow installation required for conversion
- Secure: no code execution during parsing phase

### 2. **Dependency Analysis & Topological Sorting**
- Maps Langflow edges to Stepflow step dependencies
- Uses Kahn's algorithm for proper execution ordering
- Handles complex dependency graphs with validation

### 3. **Schema Discovery**
- Extracts component input/output schemas from JSON metadata
- Maps Langflow types to Stepflow type system
- Preserves native type semantics

### 4. **UDF Component Execution**
- Stores original Langflow component code as blobs
- Executes via `UDFExecutor` for 100% compatibility
- Supports bidirectional communication with Stepflow runtime

### 5. **Type Preservation**
- Maintains Langflow `Message`, `Data`, `DataFrame` types
- Handles complex nested object serialization
- Automatic type conversion between systems

## 📦 Supported Components

The integration supports all Langflow component types through UDF execution:

### Compatibility Matrix

| Component Type | Support Status | Conversion Method |
|---------------|----------------|-------------------|
| ChatInput | ✅ Full | UDF Executor |
| ChatOutput | ✅ Full | UDF Executor |
| PromptTemplate | ✅ Full | UDF Executor |
| LanguageModelComponent | ✅ Full | UDF Executor |
| OpenAI/Anthropic Models | ✅ Full | UDF Executor |
| Agent Components | ✅ Full | UDF Executor |
| Calculator/Tool Components | ✅ Full | UDF Executor |
| Memory Components | ✅ Full | UDF Executor |
| Vector Store/Embeddings | ✅ Full | UDF Executor |
| Text Splitter/Loaders | ✅ Full | UDF Executor |
| File Components | ✅ Full | UDF Executor |
| Custom Components | ✅ Full | UDF Executor |
| Note/Documentation | ✅ Preserved | Filtered out |

### Langflow Features Support

| Feature | Support Status | Notes |
|---------|----------------|-------|
| Component Dependencies | ✅ Full | Converted to Stepflow step dependencies |
| Environment Variables | ✅ Full | API keys and secrets passed through |
| Custom Python Components | ✅ Full | UDF execution preserves all functionality |
| Template Variables | ✅ Full | Resolved during component execution |
| Dropdown/Selection Fields | ✅ Full | Configuration preserved |
| File Uploads | ✅ Full | File content passed as input data |
| Multi-output Components | ✅ Full | Output selection preserved |

## 🧪 Testing & Development

### Test Suite Overview

The integration includes comprehensive testing across multiple levels:

- **Unit Tests**: Component conversion, dependency analysis, UDF execution
- **Integration Tests**: Full workflow conversion and validation
- **Real Execution Tests**: End-to-end testing with actual Langflow components
- **Mock Testing**: Safe testing without external dependencies

### Running Tests

```bash
# Run all tests
uv run pytest

# Run with coverage
uv run pytest --cov=stepflow_langflow_integration --cov-report=html

# Run specific test categories
uv run pytest tests/unit/                    # Unit tests only
uv run pytest tests/integration/ -m "not slow"  # Fast integration tests
uv run pytest tests/integration/ -m "real_execution"  # Real execution tests

# Run the complete test suite (as used in CI)
../../../scripts/test-langflow-integration.sh
```

### Development Setup

```bash
# Development installation with all dependencies
uv sync --dev

# Code formatting
uv run ruff format

# Linting
uv run ruff check --fix

# Type checking
uv run mypy src tests

# Run development server
uv run stepflow-langflow serve
```

## 🔮 Future Improvements

### Planned Features
- **HTTP Transport**: Support for remote Langflow component servers
- **Streaming Support**: Real-time workflow execution updates
- **Performance Optimizations**: Parallel component execution, caching
- **Enhanced Type System**: Better Langflow ↔ Stepflow type mapping

### Recent Improvements
- **Extended Agent Timeout**: Agent workflow execution timeout increased from 30s to 180s to accommodate complex agent loops and tool sequences
- **Agent Execution Optimization**: Enhanced Agent component handling with database bypass and memory isolation for Stepflow environment

### Potential Enhancements
- **Debug Tools**: Enhanced workflow debugging and profiling
- **Agent Execution Refinement**: Further optimization of OpenAI Agent execution reliability
- **Native Component Mappings**: Direct Stepflow equivalents for common components (performance optimization)

## 📚 Examples

The `tests/fixtures/langflow/` directory contains example workflows from official Langflow starter projects. **All examples work successfully in `--mock` mode** (7/7) and **6 out of 7 workflows work in real execution mode** with API keys configured.

### 🚀 Quick Test Commands

All workflows accept a `"message"` field as their primary input. **Integration Status: 6/7 workflows fully working with real execution.**

#### Mock Mode Testing (Safe - No API Keys Required)
```bash
# All 7 workflows work perfectly in mock mode
# 1. Simple Chat - Basic input/output flow ✅
uv run stepflow-langflow execute tests/fixtures/langflow/simple_chat.json \
  '{"message": "Hello!"}' --mock

# 2. Basic Prompting - LLM prompting with templates ✅
uv run stepflow-langflow execute tests/fixtures/langflow/basic_prompting.json \
  '{"message": "Write a haiku about coding"}' --mock

# 3. Memory Chatbot - Conversational AI with memory ✅
uv run stepflow-langflow execute tests/fixtures/langflow/memory_chatbot.json \
  '{"message": "Remember my name is Alice"}' --mock

# 4. Document Q&A - Document-based question answering ✅
uv run stepflow-langflow execute tests/fixtures/langflow/document_qa.json \
  '{"message": "What is the main topic of this document?"}' --mock

# 5. Simple Agent - AI agent with calculator/tools ✅
uv run stepflow-langflow execute tests/fixtures/langflow/simple_agent.json \
  '{"message": "Calculate 15 * 23 and explain the result"}' --mock

# 6. Vector Store RAG - Complex retrieval augmented generation ✅
uv run stepflow-langflow execute tests/fixtures/langflow/vector_store_rag.json \
  '{"message": "Find information about artificial intelligence"}' --mock

# 7. OpenAI Chat - Direct OpenAI API integration ✅
uv run stepflow-langflow execute tests/fixtures/langflow/openai_chat.json \
  '{"message": "What is Python programming?"}' --mock
```

#### Real Execution Testing (Requires API Keys)
```bash
# Working workflows (6/7) - Remove --mock for real execution
# Requires OPENAI_API_KEY environment variable

# 1. Simple Chat - Direct passthrough workflow ✅ WORKING
uv run stepflow-langflow execute tests/fixtures/langflow/simple_chat.json \
  '{"message": "Hello from real execution!"}'

# 2. Basic Prompting - LLM with template processing ✅ WORKING
uv run stepflow-langflow execute tests/fixtures/langflow/basic_prompting.json \
  '{"message": "Write a haiku about coding"}'

# 3. Memory Chatbot - Conversational AI with memory ✅ WORKING
uv run stepflow-langflow execute tests/fixtures/langflow/memory_chatbot.json \
  '{"message": "Remember my name is Alice"}'

# 4. Document Q&A - Document-based question answering ✅ WORKING
uv run stepflow-langflow execute tests/fixtures/langflow/document_qa.json \
  '{"message": "What is the main topic of this document?"}'

# 5. Simple Agent - AI agent with calculator/tools ⚠️ PARTIAL (OpenAI Agent execution issues)
# Note: Architecture works, but Agent execution may timeout. Use extended timeout:
uv run stepflow-langflow execute tests/fixtures/langflow/simple_agent.json \
  '{"message": "Calculate 15 * 23 and explain the result"}' --timeout 180

# 6. Vector Store RAG - Complex retrieval augmented generation ✅ WORKING
uv run stepflow-langflow execute tests/fixtures/langflow/vector_store_rag.json \
  '{"message": "Find information about artificial intelligence"}'

# 7. OpenAI Chat - Direct OpenAI API integration ✅ WORKING
uv run stepflow-langflow execute tests/fixtures/langflow/openai_chat.json \
  '{"message": "What is Python programming?"}'
```

### 💡 Workflow Input Patterns & Status

All workflows expect JSON input with a `message` field:

| Workflow | Input Example | Description | Real Execution Status |
|----------|---------------|-------------|----------------------|
| `simple_chat.json` | `{"message": "Hello there!"}` | Basic greeting or question | ✅ Working |
| `basic_prompting.json` | `{"message": "Write a poem about nature"}` | Creative prompts for LLM | ✅ Working |
| `memory_chatbot.json` | `{"message": "Remember I like coffee"}` | Conversational input with memory | ✅ Working |
| `document_qa.json` | `{"message": "Summarize this document"}` | Questions about documents | ✅ Working |
| `simple_agent.json` | `{"message": "Calculate 42 * 17"}` | Tasks requiring tools/calculations | ⚠️ Partial (use --timeout 180) |
| `vector_store_rag.json` | `{"message": "Explain machine learning"}` | Knowledge retrieval queries | ✅ Working |
| `openai_chat.json` | `{"message": "Explain quantum computing"}` | Direct chat with LLM | ✅ Working |

### 🔍 Workflow Analysis

Understand workflow structure before execution:

```bash
# Analyze workflow complexity and dependencies
uv run stepflow-langflow analyze tests/fixtures/langflow/vector_store_rag.json

# View converted workflow without execution
uv run stepflow-langflow execute tests/fixtures/langflow/basic_prompting.json --dry-run

# Keep temporary files for debugging
uv run stepflow-langflow execute tests/fixtures/langflow/memory_chatbot.json \
  '{"message": "Test"}' --mock --keep-files --output-dir ./debug
```

### 🎯 Production Execution Status

**Integration Status: 6/7 workflows fully operational in production.**

#### ✅ Fully Working Workflows (Real Execution)
The following workflows work reliably with real API execution:
- `simple_chat.json` - Direct passthrough workflow
- `basic_prompting.json` - LLM with template processing
- `memory_chatbot.json` - Conversational AI with memory
- `document_qa.json` - Document-based question answering
- `vector_store_rag.json` - Complex retrieval augmented generation
- `openai_chat.json` - Direct OpenAI API integration

#### ⚠️ Partially Working Workflows
- `simple_agent.json` - Architecture complete but OpenAI Agent execution issues may cause timeouts
  - **Workaround**: Use extended timeout (`--timeout 180`) and validate with `--mock` first

#### Environment Requirements
```bash
# Required environment variable for real execution
export OPENAI_API_KEY="your-openai-api-key-here"

# Optional: For vector store workflows requiring AstraDB
export ASTRA_DB_API_ENDPOINT="your-astra-endpoint"
export ASTRA_DB_APPLICATION_TOKEN="your-astra-token"
```

#### Quick Production Test
```bash
# Test a working workflow with real API
uv run stepflow-langflow execute tests/fixtures/langflow/openai_chat.json \
  '{"message": "Hello, this is a production test!"}'

# Test with extended timeout for complex workflows
uv run stepflow-langflow execute tests/fixtures/langflow/vector_store_rag.json \
  '{"message": "Explain deep learning"}' --timeout 120
```

## 🏛️ Architecture Notes

### Design Principles
- **Security First**: No arbitrary code execution during conversion
- **Compatibility**: Maintain full Langflow component behavior
- **Performance**: Efficient conversion and execution
- **Testability**: Comprehensive test coverage with multiple execution modes

### Key Components
- **`LangflowConverter`**: Core conversion logic with topological sorting
- **`UDFExecutor`**: Langflow component code execution engine
- **`DependencyAnalyzer`**: Workflow dependency graph analysis
- **`StepflowBinaryRunner`**: Integration testing utilities

### Integration Points
- **Stepflow Protocol**: JSON-RPC communication with Stepflow runtime
- **Blob Storage**: Component code and data persistence
- **Type System**: Seamless type conversion between systems
- **Error Handling**: Robust error propagation and reporting

## 📄 License

This project follows the same Apache 2.0 license as the main Stepflow project.

## 🆘 Support

- **Documentation**: This README and inline code documentation
- **Issues**: Report bugs via [GitHub Issues](https://github.com/stepflow-ai/stepflow/issues)
- **Examples**: Comprehensive examples in `tests/fixtures/langflow/`
- **Testing**: Use `--mock` mode for safe experimentation