# Stepflow Langflow Integration

This directory contains a comprehensive Python package for integrating Langflow workflows with Stepflow, providing both conversion and execution capabilities.

## Overview

The Stepflow Langflow Integration is designed as a clean, modular Python package that enables:

1. **Conversion**: Transform Langflow JSON workflows into Stepflow YAML workflows
2. **Execution**: Run Langflow components within Stepflow using native Langflow types
3. **Schema Discovery**: Automatically extract and map component schemas between formats
4. **Bidirectional Compatibility**: Preserve full Langflow semantics while leveraging Stepflow's execution engine

## Architecture

### Core Components

```
integrations/langflow/
├── pyproject.toml              # Python package configuration
├── src/
│   └── stepflow_langflow_integration/
│       ├── __init__.py         # Package exports
│       ├── converter/          # Langflow → Stepflow conversion
│       │   ├── __init__.py
│       │   ├── translator.py   # Main conversion logic
│       │   ├── schema_mapper.py # Schema discovery and mapping
│       │   └── dependency_analyzer.py # Workflow dependency analysis
│       ├── executor/           # Stepflow execution components
│       │   ├── __init__.py
│       │   ├── langflow_server.py # Stepflow component server
│       │   ├── udf_executor.py # Execute Langflow code as UDFs
│       │   └── type_converter.py # Langflow ↔ Stepflow type conversion
│       ├── cli/                # Command-line interface
│       │   ├── __init__.py
│       │   ├── convert.py      # Conversion commands
│       │   └── serve.py        # Server management
│       ├── types/              # Type definitions
│       │   ├── __init__.py
│       │   ├── langflow_types.py # Langflow type wrappers
│       │   └── stepflow_types.py # Stepflow type mappings
│       └── utils/              # Shared utilities
│           ├── __init__.py
│           ├── environment.py  # Environment variable handling
│           └── validation.py   # Workflow validation
├── tests/                      # Comprehensive test suite
│   ├── __init__.py
│   ├── fixtures/               # Test workflow files
│   ├── test_converter/         # Conversion tests
│   ├── test_executor/          # Execution tests
│   └── test_integration/       # End-to-end tests
└── examples/                   # Usage examples
    ├── basic_conversion.py
    ├── custom_components.py
    └── workflows/              # Example Langflow workflows
```

## Key Features

### 1. JSON-First Conversion Engine

- **Direct JSON Parsing**: Fast, secure parsing of Langflow JSON workflows
- **Full Langflow Support**: Handles all standard Langflow component types and structures
- **Schema Discovery**: Intelligent extraction of component schemas from JSON metadata
- **Dependency Analysis**: Correctly maps Langflow edges to Stepflow step dependencies
- **Configuration Mapping**: Translates Langflow parameters to Stepflow inputs
- **Lightweight**: Minimal dependencies, no Langflow installation required
- **Secure**: No risk of code execution from malicious JSON files

### 2. Execution Framework

- **Native Type Support**: Preserves Langflow `Message`, `Data`, and `DataFrame` semantics
- **UDF Execution**: Runs original Langflow component code for 100% compatibility  
- **Environment Integration**: Handles API keys and configuration via environment variables
- **Error Handling**: Provides comprehensive error reporting and graceful fallbacks

### 3. Developer Experience

- **CLI Tools**: Easy-to-use command-line interface for conversion and execution
- **Python API**: Programmatic access for custom integrations
- **Type Safety**: Full type hints and validation throughout
- **Testing**: Comprehensive test suite with real Langflow workflows

## Installation

```bash
cd integrations/langflow
pip install -e .

# Or with uv
uv pip install -e .
```

## Usage Examples

### Command Line Conversion

```bash
# Convert Langflow JSON to Stepflow YAML
stepflow-langflow convert workflow.json output.yaml

# Convert with options
stepflow-langflow convert workflow.json output.yaml --pretty           # Pretty-printed YAML
stepflow-langflow convert workflow.json output.yaml --validate         # Validate schemas

# Analyze workflow structure  
stepflow-langflow analyze workflow.json
```

### Python API

```python
from stepflow_langflow_integration import LangflowConverter, StepflowLangflowServer

# Convert workflow
converter = LangflowConverter()
stepflow_yaml = converter.convert_file("workflow.json")

# Convert with validation
converter = LangflowConverter(validate_schemas=True)
stepflow_yaml = converter.convert_file("workflow.json")

# Programmatic conversion
with open("workflow.json", "r") as f:
    langflow_json = json.load(f)
    
stepflow_workflow = converter.convert(langflow_json)

# Start component server
server = StepflowLangflowServer()
server.run()
```

### Stepflow Configuration

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

## Component Types

### 1. UDF Executor

Executes original Langflow component code with full compatibility:

```yaml
- id: langflow_chat
  component: langflow://udf_executor
  input:
    code: "class ChatInput(Component): ..."
    template: {...}
    component_type: "ChatInput"
    runtime_inputs:
      message: "Hello, world!"
```

### 2. Native Components

Direct Stepflow implementations of common Langflow components:

```yaml
- id: openai_chat
  component: langflow://openai_chat
  input:
    api_key: "${OPENAI_API_KEY}"
    model: "gpt-4"
    messages: { $from: { step: previous_step }, path: "messages" }
```

### 3. Fusion Components

Multi-component chains that preserve complex object semantics:

```yaml
- id: text_processing_chain
  component: langflow://fusion_executor
  input:
    components: [...]
    execution_chain: ["TextSplitter", "Embeddings", "VectorStore"]
```

## Type System

### Langflow Types

The integration preserves native Langflow types:

```python
# Message type
{
  "text": "Hello, world!",
  "sender": "user", 
  "sender_name": "User",
  "type": "Message"
}

# Data type
{
  "data": {"key": "value"},
  "text_key": "text",
  "type": "Data"
}

# DataFrame type
{
  "data": [{"col1": "val1"}],
  "type": "DataFrame"
}
```

### Schema Mapping

Automatic schema discovery and conversion:

- **Langflow JSON metadata** → **Stepflow JSON Schema**
- **Component outputs** → **Step output schemas**
- **Template fields** → **Step input schemas**

## Development Roadmap

### Phase 1: Foundation (Current)
- [x] Project structure and packaging
- [ ] Core converter implementation
- [ ] Basic UDF executor
- [ ] CLI interface
- [ ] Initial test suite

### Phase 2: Feature Complete
- [ ] Advanced schema mapping
- [ ] Native component implementations
- [ ] Fusion component support
- [ ] Comprehensive error handling
- [ ] Performance optimizations

### Phase 3: Production Ready
- [ ] Production testing with real workflows
- [ ] Performance benchmarking
- [ ] Documentation and examples
- [ ] CI/CD integration
- [ ] Community feedback integration

### Phase 4: Advanced Features
- [ ] Visual workflow editor integration
- [ ] Hybrid execution modes
- [ ] Component marketplace integration
- [ ] Advanced debugging tools
- [ ] Workflow optimization suggestions

## Conversion Strategy: JSON-First Approach

### Why JSON-First?

After analyzing both direct JSON parsing and Langflow's Graph API, we chose the **JSON-first approach** for its simplicity and practical benefits:

| Aspect | JSON-First | Graph API |
|--------|------------|-----------|
| **Speed** | Fast ⚡ | Slow 🐌 |
| **Dependencies** | Minimal 📦 | Heavy 🏋️ |
| **Security** | Safe 🔒 | RCE Risk ⚠️ |
| **Implementation** | Simple ✨ | Complex 🔧 |
| **Maintenance** | Low 👍 | High 🔄 |

### How It Works

```python
class LangflowConverter:
    def __init__(self, validate_schemas=False):
        """
        validate_schemas: Perform additional schema validation
        """
        self.validate_schemas = validate_schemas
    
    def convert(self, langflow_data: Dict) -> StepflowWorkflow:
        # 1. Extract nodes and edges from JSON
        nodes = langflow_data["data"]["nodes"]
        edges = langflow_data["data"]["edges"]
        
        # 2. Build dependency graph
        dependencies = self._build_dependency_graph(edges)
        
        # 3. Convert each node to Stepflow step
        steps = self._convert_nodes_to_steps(nodes, dependencies)
        
        # 4. Generate Stepflow workflow
        return StepflowWorkflow(steps=steps, ...)
```

### What We Parse

The JSON-first approach extracts everything we need directly from the Langflow JSON structure:

- **Nodes**: Component definitions with templates and metadata
- **Edges**: Connections between components for dependency analysis  
- **Schema Information**: Output types from component metadata
- **Configuration**: Template parameters and default values
- **Environment Variables**: API keys and secrets handling

### Proven Success

This approach is based on the working prototype in `fraz/translation-layer` that successfully handles:
- ✅ Standard Langflow components (ChatInput, ChatOutput, LanguageModel, etc.)
- ✅ Complex multi-component workflows
- ✅ Component fusion for performance optimization
- ✅ Schema discovery and type mapping
- ✅ Environment variable resolution

## Design Principles

### 1. Compatibility First
- **100% Langflow Compatibility**: Execute original component code unchanged
- **Preserve Semantics**: Maintain all Langflow type behaviors and edge cases
- **Graceful Degradation**: Fallback to UDF execution when native conversion fails

### 2. Performance Minded  
- **Lazy Loading**: Import Langflow dependencies only when needed
- **Efficient Conversion**: Minimize overhead in translation process
- **Memory Management**: Handle large workflows with streaming where possible

### 3. Developer Friendly
- **Clear APIs**: Intuitive Python interfaces with comprehensive documentation
- **Rich Diagnostics**: Detailed error messages and conversion analysis
- **Extensible Design**: Easy to add support for new Langflow components

### 4. Production Ready
- **Robust Error Handling**: Comprehensive error recovery and reporting  
- **Security Conscious**: Safe execution of user code with proper sandboxing
- **Scalable Architecture**: Support for high-throughput production workloads

## Contributing

### Development Setup

```bash
# Clone the repository
cd integrations/langflow

# Install development dependencies
uv sync --dev

# Install pre-commit hooks
pre-commit install

# Run tests
pytest

# Run linting
ruff check
ruff format
```

### Adding New Components

1. **Native Implementation**: Create direct Stepflow component
2. **Schema Mapping**: Add type conversion rules
3. **Tests**: Comprehensive test coverage
4. **Documentation**: Update examples and README

### Testing Strategy

- **Unit Tests**: Individual component conversion and execution
- **Integration Tests**: End-to-end workflow conversion and execution
- **Compatibility Tests**: Verify identical behavior to native Langflow
- **Performance Tests**: Benchmark conversion and execution speed

## Related Projects

- **[Stepflow Core](../../stepflow-rs/)**: Main Stepflow execution engine
- **[Stepflow Python SDK](../../sdks/python/)**: Core Python SDK for component development
- **[Langflow](https://github.com/langflow-ai/langflow)**: Original Langflow project

## License

This project follows the same license as the main Stepflow project.

## Support

- **Issues**: Report bugs and feature requests via GitHub Issues
- **Discussions**: Join the community discussions for questions and ideas
- **Documentation**: Full API documentation available in the `docs/` directory