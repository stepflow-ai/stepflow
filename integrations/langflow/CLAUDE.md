# Langflow Integration Development Guide

This guide covers development-specific information for the Stepflow Langflow integration.

See the root `/CLAUDE.md` for general project overview and the `README.md` for user-facing documentation.

## Overview

The Langflow integration enables seamless conversion and execution of Langflow workflows within Stepflow. It consists of two main components:

1. **Converter**: Translates Langflow JSON workflows to Stepflow YAML format
2. **Executor**: Component server that runs real Langflow components via the Stepflow runtime

## Architecture

### Directory Structure

```
integrations/langflow/
├── src/stepflow_langflow_integration/
│   ├── cli/                    # CLI commands (execute, convert, analyze, etc.)
│   ├── converter/              # Langflow → Stepflow conversion
│   │   ├── translator.py       # Main conversion logic
│   │   ├── dependency_analyzer.py  # Workflow dependency analysis
│   │   ├── node_processor.py   # Node-to-step conversion
│   │   └── schema_mapper.py    # Schema translation
│   ├── executor/               # Component execution
│   │   ├── langflow_server.py  # Stepflow component server
│   │   ├── udf_executor.py     # UDF component execution
│   │   ├── standalone_server.py # HTTP server entry point
│   │   └── type_converter.py   # Type conversion utilities
│   └── components/             # Component implementations
│       └── component_tool.py   # Component wrapper utilities
├── tests/
│   ├── unit/                   # Unit tests (no external dependencies)
│   ├── integration/            # Integration tests (REQUIRES API KEYS)
│   ├── fixtures/langflow/      # Example Langflow workflows
│   └── helpers/                # Test utilities
└── pyproject.toml              # Project configuration
```

### Key Design Patterns

**Pre-compilation Architecture**: The executor uses a pre-compilation approach to avoid JSON-RPC deadlocks. All component blobs are fetched and compiled before execution begins, eliminating the need for context calls during component execution.

**Tweaks System**: Runtime configuration overrides for component parameters (API keys, model settings, etc.) without modifying workflow files.

**Dependency Analysis**: Automatic topological sorting of Langflow nodes to determine correct execution order in Stepflow.

**Type Preservation**: Full type conversion between Langflow's Python types and Stepflow's ValueExpr system.

## Development Commands

### Setup

```bash
# Install dependencies (development mode)
cd integrations/langflow
uv sync --dev

# Install with HTTP server support
uv sync --dev --extra http
```

### Testing

```bash
# Run unit tests (no API keys required)
uv run pytest tests/unit/

# Run integration tests (REQUIRES OPENAI_API_KEY)
export OPENAI_API_KEY="sk-your-key-here"
uv run pytest tests/integration/

# Run all tests
uv run pytest

# Run with coverage
uv run pytest --cov=stepflow_langflow_integration --cov-report=html

# Run specific test file
uv run pytest tests/unit/test_dependency_analyzer.py -v

# Run tests matching a pattern
uv run pytest -k "test_conversion" -v
```

### Code Quality

```bash
# Format code
uv run poe fmt

# Check formatting
uv run poe fmt-check

# Lint code (auto-fix)
uv run poe lint-fix

# Lint check (no changes)
uv run poe lint-check

# Type checking
uv run poe typecheck

# Dependency checking
uv run poe dep-check

# Run all checks
uv run poe check
```

### Running the Integration

```bash
# Execute a Langflow workflow
uv run stepflow-langflow execute tests/fixtures/langflow/basic_prompting.json \
  '{"message": "Write a haiku"}' \
  --tweaks '{"LanguageModelComponent-kBOja": {"api_key": "sk-..."}}'

# Start component server (STDIO mode)
uv run stepflow-langflow serve

# Start component server (HTTP mode)
uv run stepflow-langflow-server --host 0.0.0.0 --port 8080

# Convert workflow (no execution)
uv run stepflow-langflow convert workflow.json output.yaml

# Analyze workflow structure
uv run stepflow-langflow analyze workflow.json
```

## Environment Setup

### API Keys for Integration Tests

Integration tests execute real Langflow components and require API credentials:

```bash
# Copy the example environment file
cp env.example .env

# Edit .env and add your API keys
OPENAI_API_KEY=sk-your-actual-key-here
```

**Required for integration tests**:
- `OPENAI_API_KEY`: OpenAI API key for LLM components

**Optional** (for other providers):
- `ANTHROPIC_API_KEY`: Claude models
- `GOOGLE_API_KEY`: Google AI models
- `HUGGINGFACE_API_TOKEN`: HuggingFace models
- `COHERE_API_KEY`: Cohere models

**Important**: Integration tests will fail without proper API keys set. This is expected behavior - they test real component execution, not mock behavior.

### Running Tests Without API Keys

Unit tests do not require API keys and can be run independently:

```bash
# Run only unit tests (no API keys needed)
uv run pytest tests/unit/
```

## Component Server Modes

The Langflow integration provides a Stepflow component server that can run in two modes:

### STDIO Mode (Default)

Process-based communication for local development:

```bash
# Run server in STDIO mode
uv run stepflow-langflow serve

# Or use the Python module directly
uv run python -m stepflow_langflow_integration.executor.standalone_server
```

**Configuration** (in stepflow-config.yml):
```yaml
plugins:
  langflow:
    type: stepflow
    command: uv
    args: ["--project", "integrations/langflow", "run", "stepflow-langflow", "serve"]

routes:
  "/langflow/{*component}":
    - plugin: langflow
```

### HTTP Mode

HTTP-based communication for distributed deployments:

```bash
# Start HTTP server
uv run stepflow-langflow-server --host 0.0.0.0 --port 8080

# Custom configuration
uv run stepflow-langflow-server --host localhost --port 9000 --workers 4
```

**Configuration** (in stepflow-config.yml):
```yaml
plugins:
  langflow_http:
    type: stepflow
    url: "http://localhost:8080"

routes:
  "/langflow/{*component}":
    - plugin: langflow_http
```

## Testing Strategy

### Unit Tests (`tests/unit/`)

Test individual components in isolation without external dependencies:

- `test_dependency_analyzer.py`: Workflow dependency analysis
- `test_schema_mapper.py`: Schema conversion logic
- `test_type_converter.py`: Type conversion utilities
- `test_udf_executor.py`: UDF execution logic
- `test_stepflow_tweaks.py`: Tweaks application

**No API keys required** - uses mocks and fixtures.

### Integration Tests (`tests/integration/`)

Test real workflow execution with actual Langflow components:

- Requires `OPENAI_API_KEY` environment variable
- Uses example workflows from `tests/fixtures/langflow/`
- Tests end-to-end conversion and execution
- Validates output structure and content

**Requires API keys** - executes real components.

### Test Markers

```bash
# Run only fast tests (skip slow integration tests)
uv run pytest -m "not slow"

# Run only integration tests
uv run pytest -m integration

# Run tests that use real Langflow execution
uv run pytest -m real_execution
```

## Example Workflows

The `tests/fixtures/langflow/` directory contains example workflows from official Langflow starter projects:

| Workflow | Description | Components |
|----------|-------------|------------|
| `basic_prompting.json` | Simple LLM with prompt | ChatInput, PromptTemplate, OpenAI |
| `memory_chatbot.json` | Conversational AI | ChatInput, Memory, OpenAI, ChatOutput |
| `document_qa.json` | Document Q&A | Document, Embeddings, VectorStore, OpenAI |
| `vector_store_rag.json` | RAG pipeline | Documents, Embeddings, ChromaDB, OpenAI |
| `simple_agent.json` | AI agent with tools | ChatInput, Agent, Tools, OpenAI |

All workflows accept a `message` field as input:
```json
{"message": "Write a haiku about coding"}
```

### Updating Example Workflows

```bash
# Update fixtures from latest Langflow starter projects
cd tests/fixtures/langflow
uv run python update_fixtures.py
```

## Conversion Process

The conversion pipeline transforms Langflow JSON to Stepflow YAML:

1. **Parse**: Load and validate Langflow JSON structure
2. **Analyze**: Build dependency graph and execution order
3. **Map Schemas**: Convert Langflow schemas to Stepflow schemas
4. **Process Nodes**: Transform each node to a Stepflow step
5. **Build Flow**: Construct final Flow object using FlowBuilder
6. **Serialize**: Convert to YAML with proper formatting

### Node → Step Mapping

Each Langflow node becomes a Stepflow step:

```python
# Langflow node
{
  "id": "ChatInput-abc123",
  "type": "ChatInput",
  "data": {
    "template": {"input_value": "..."}
  }
}

# Becomes Stepflow step
steps:
  ChatInput-abc123:
    component: /langflow/udf_executor
    input:
      node_id: ChatInput-abc123
      component_type: ChatInput
      # ... other fields
```

### Tweaks Application

Tweaks modify component configurations at execution time:

```python
# Original node data
{
  "api_key": {"value": ""},
  "temperature": {"value": 0.7}
}

# Applied tweaks
{
  "LanguageModelComponent-xyz": {
    "api_key": "sk-actual-key",
    "temperature": 0.9
  }
}

# Result
{
  "api_key": {"value": "sk-actual-key"},
  "temperature": {"value": 0.9}
}
```

## Troubleshooting

### Integration Tests Failing

**Symptom**: Tests fail with `"Environment variable 'OPENAI_API_KEY' for 'api_key' not set."`

**Solution**: Set the required environment variable:
```bash
export OPENAI_API_KEY="sk-your-key-here"
uv run pytest tests/integration/
```

**Expected**: Integration tests REQUIRE real API keys and will fail without them.

### Component Execution Timeout

**Symptom**: `simple_agent.json` times out during execution

**Solution**: Use longer timeout for complex workflows:
```bash
uv run stepflow-langflow execute tests/fixtures/langflow/simple_agent.json \
  '{"message": "Calculate 42 * 17"}' \
  --timeout 180
```

### Import Errors

**Symptom**: `ImportError: cannot import name 'X' from 'stepflow_worker'`

**Solution**: Ensure stepflow-worker is installed in editable mode:
```bash
cd ../../sdks/python
uv sync
cd ../../integrations/langflow
uv sync --dev
```

### Type Errors

**Symptom**: `mypy` reports type errors in generated code

**Solution**: Ignore errors in tests (already configured):
```toml
[[tool.mypy.overrides]]
module = "tests.*"
ignore_errors = true
```

### JSON-RPC Deadlocks

**Symptom**: Execution hangs during component execution

**Solution**: The executor uses pre-compilation to prevent this. If it occurs:
1. Check that all blob IDs are pre-fetched before execution
2. Verify no synchronous context calls during component execution
3. Review `udf_executor.py` for proper pre-compilation flow

## Code Conventions

### Imports

Follow standard Python import ordering:
```python
# Standard library
import json
from pathlib import Path

# Third-party
import msgspec
from stepflow_worker import Flow, FlowBuilder

# Local
from ..exceptions import ConversionError
from .dependency_analyzer import DependencyAnalyzer
```

### Type Hints

Use type hints consistently:
```python
def convert(self, langflow_data: dict[str, Any]) -> Flow:
    """Convert Langflow data to Stepflow workflow."""
    ...
```

### Error Handling

Use custom exceptions from `exceptions.py`:
```python
from ..exceptions import ConversionError

if "data" not in langflow_data:
    raise ConversionError("Missing 'data' field in Langflow workflow")
```

### Logging

Use standard logging module:
```python
import logging

logger = logging.getLogger(__name__)

logger.info("Converting workflow with %d nodes", node_count)
logger.warning("Potential issue detected: %s", issue)
logger.error("Conversion failed: %s", error)
```

## Dependencies

### Core Dependencies

- `stepflow-worker`: Stepflow Python SDK (local editable install)
- `langflow-nightly`: Langflow library for component execution
- `lfx-nightly`: Langflow extensions
- `pyyaml`: YAML serialization
- `msgspec`: Fast JSON/msgpack serialization
- `click`: CLI framework
- `pydantic`: Data validation
- `python-dotenv`: Environment variable loading

### Development Dependencies

- `pytest`: Testing framework
- `pytest-asyncio`: Async test support
- `pytest-cov`: Coverage reporting
- `pytest-mock`: Mocking utilities
- `mypy`: Type checking
- `ruff`: Linting and formatting
- `deptry`: Dependency checking
- `poethepoet`: Task runner

## Python Version Support

Supports Python 3.11, 3.12, and 3.13. All versions are tested in CI.

### Testing a Specific Version

```bash
# Install specific Python version
uv python install 3.11  # or 3.12, 3.13

# Pin the version
uv python pin 3.11

# Sync dependencies and run tests
uv sync --dev
uv run pytest
```

## CI/CD Integration

The integration is tested as part of the main Stepflow CI pipeline:

```bash
# Script used by CI (from repo root)
./scripts/check-langflow.sh
```

This script:
1. Syncs langflow integration dependencies
2. Runs all tests (unit + integration)
3. Expects `OPENAI_API_KEY` to be set in CI environment

**Note**: Integration test failures in CI due to missing API keys should be investigated to ensure the environment variable is properly configured.

## Related Documentation

- Root `/CLAUDE.md`: General Stepflow development guide
- `/sdks/python/CLAUDE.md`: Python SDK development guide
- `README.md`: User-facing documentation and CLI reference
- `stepflow-rs/CLAUDE.md`: Rust development guide (for core runtime)
