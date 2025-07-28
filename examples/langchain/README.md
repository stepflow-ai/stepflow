# LangChain Integration Example

This example demonstrates comprehensive integration of LangChain with StepFlow, showcasing **three practical approaches** for using LangChain runnables as StepFlow components.

## Overview

The LangChain integration provides three different ways to use LangChain runnables:

1. **Decorated Runnable**: Use `@server.langchain_component` decorator to register factories as StepFlow components
2. **Named Runnable**: Execute runnables directly by import path using `/invoke_named` component
3. **UDF**: Store Python code in blobs that creates LangChain runnables, executed via `/udf`

## Prerequisites

- Rust (for building StepFlow)
- Python 3.11+ with uv
- LangChain core library

## Setup

1. **Install dependencies**:
   ```bash
   cd ../../sdks/python
   uv add --group dev langchain-core
   ```

2. **Build StepFlow**:
   ```bash
   cd ../../stepflow-rs
   cargo build
   ```

## Running the Examples

Each approach has its own dedicated workflow file for focused demonstration:

### Decorated Runnable

```bash
# Demonstrates @server.langchain_component decorator pattern
../../stepflow-rs/target/debug/stepflow run \
  --flow=decorated_runnable.yaml \
  --input=input.json \
  --config=stepflow-config.yml
```

### Named Runnable

```bash
# Demonstrates direct invocation with import paths via /invoke_named
../../stepflow-rs/target/debug/stepflow run \
  --flow=named_runnable.yaml \
  --input=input.json \
  --config=stepflow-config.yml
```

### UDF

```bash
# Demonstrates self-contained Python code creating runnables via /udf
../../stepflow-rs/target/debug/stepflow run \
  --flow=udf.yaml \
  --input=input.json \
  --config=stepflow-config.yml
```



## Three Integration Approaches Explained

### Decorated Runnable (`@server.langchain_component`)

Components are registered directly with the server using decorators:

```python
@server.langchain_component(name="text_analyzer")
def create_text_analyzer():
    """Analyze text and return various metrics."""
    def analyze_text(text_input):
        text = text_input["text"]
        return {
            "word_count": len(text.split()),
            "char_count": len(text),
            # ... more analysis
        }
    return RunnableLambda(analyze_text)
```

**Usage in workflow:**
```yaml
- id: analyze_text
  component: /text_analyzer
  input:
    input:
      text: "Your text here"
    execution_mode: invoke
```

**Components available:**
- `/text_analyzer`: Text analysis (word count, char count, etc.)
- `/sentiment_classifier`: Sentiment analysis using keyword matching
- `/math_operations`: Parallel mathematical operations

### Named Runnable (via `/invoke_named` with Import Paths)

Runnables are imported from Python modules and executed directly using standard import path syntax:

```python
# Create runnables in a module (example_runnables.py)
from langchain_core.runnables import RunnableLambda

def process_text(data):
    text = data["text"]
    words = text.split()
    return {
        "processed_text": " ".join(word.capitalize() for word in words),
        "word_count": len(words),
        "original_length": len(text)
    }

# Make the runnable available for import
text_processor = RunnableLambda(process_text)

# Use the SDK-provided invoke_named component with caching
from stepflow_py import create_invoke_named_component
create_invoke_named_component(server)
```

**Usage in workflow:**
```yaml
# Directly invoke runnable from import path in a single step
- id: process_text
  component: /invoke_named
  input:
    import_path: example_runnables.text_processor
    input:
      text: "Your text here"
```

**Import path examples:**
- `example_runnables.text_processor`: Module.attribute syntax
- `mypackage.submodule:my_runnable`: Module:attribute syntax (alternative)
- `utils.text.capitalizer`: Nested module paths

**Benefits:**
- Single-step execution (no serialization/deserialization overhead)
- Automatic caching by import path (similar to UDF blob IDs)
- Much simpler workflow structure
- Direct execution without intermediate steps
- Provided by core SDK - no need to reimplement in each project
- Cache can be cleared with `clear_import_cache()` if needed

**SDK Functions:**
```python
from stepflow_py import (
    create_invoke_named_component,  # Add /invoke_named to your server
    invoke_named_runnable,          # Core async function for direct use
    get_runnable_from_import_path,  # Import runnables with caching
    clear_import_cache             # Clear the import cache
)

# Use in your own components
result = await invoke_named_runnable(
    import_path="mymodule.my_runnable",
    input_data={"key": "value"},
    execution_mode="invoke",
    context=stepflow_context
)
```

### UDF (Python code creating runnables)

Python code is created as a blob and executed via the `/udf` component. The workflow demonstrates a completely self-contained approach:

```yaml
# Step 1: Create blob containing Python code
- id: create_text_processor_blob
  component: /builtin/create_blob
  input:
    data:
      code: |
        from langchain_core.runnables import RunnableLambda
        
        def process_text(data):
            text = data["text"]
            # Custom processing logic here
            return {
                "processed_text": text.upper(),
                "word_count": len(text.split()),
                "processed_by": "user_udf"
            }
        
        # UDF must assign the runnable to 'result'
        result = RunnableLambda(process_text)

# Step 2: Execute the UDF code
- id: execute_udf
  component: /udf
  input:
    blob_id:
      $from: {step: create_text_processor_blob}
      path: blob_id
    input:
      text: "Your text here"
```

**Flexibility:**
- Users can create any LangChain runnable they want
- No restrictions on functionality - full Python/LangChain capabilities
- Completely self-contained - no external blob_id required
- Code is created dynamically within the workflow


## Sample Inputs

Each approach has different input requirements:

### Decorated Runnable
```json
{
  "text": "This is a fantastic example of registered LangChain components!"
}
```

### Named Runnable  
```json
{
  "text": "Process this text",
  "numbers": [1, 2, 3, 4, 5],
  "template": "Hello {name}!",
  "values": {"name": "World"}
}
```

### UDF
```json
{
  "text": "This demonstrates self-contained user-defined LangChain runnables!"
}
```


## Architecture

### Configuration

The `stepflow-config.yml` configures:
- **`langchain` plugin**: Python server running the LangChain components
- **Routing**: Components first try the langchain plugin, then fall back to builtin

### LangChain Server

The `langchain_server.py` file demonstrates:
- All three integration approaches in one server
- Component registration using different patterns
- Integration with StepFlow's bidirectional communication
- Type-safe component definitions with msgspec

### Key Features

1. **Multiple Integration Patterns**: Three practical ways to achieve the same goals
2. **Async Compatibility**: Both StepFlow and LangChain use async/await patterns  
3. **Type Safety**: msgspec integration with JSON Schema support
4. **Bidirectional Communication**: Components access StepFlow runtime via `StepflowContext`
5. **Flexible Execution**: Support for different runnable creation and execution patterns
6. **Schema Generation**: Automatic schema extraction where applicable

## When to Use Each Approach

### Decorated Runnable
- **Best for**: Stable, reusable components with fixed logic
- **Pros**: Simple to implement, automatic registration, good performance
- **Cons**: Less flexible, requires server restart for changes

### Named Runnable
- **Best for**: Reusable modules, standard library patterns, team-shared components
- **Pros**: Single-step execution, uses standard Python import syntax, promotes modular design, cacheable by import path, no serialization overhead
- **Cons**: Requires importable modules, path must be available at runtime

### UDF
- **Best for**: Dynamic code generation, user-provided logic, maximum flexibility
- **Pros**: Complete flexibility, can generate code dynamically, supports user input
- **Cons**: Security considerations, compilation overhead, more complex


## Files

### Core Files
- `langchain_server.py`: Python server demonstrating all three approaches
- `example_runnables.py`: Example module with importable runnables
- `stepflow-config.yml`: StepFlow configuration
- `input.json`: Sample input data
- `README.md`: This documentation

### Workflow Files (One per Approach)
- `decorated_runnable.yaml`: Demonstrates registered components (@server.langchain_component)
- `named_runnable.yaml`: Demonstrates direct import path invocation via /invoke_named
- `udf.yaml`: Demonstrates self-contained UDF with Python code creating runnables


## Troubleshooting

### Common Issues

1. **Missing LangChain**: Install with `uv add langchain-core`
2. **Import errors**: Ensure StepFlow Python SDK is properly installed
3. **Serialization issues**: Some complex runnables may not serialize properly
4. **Type annotation issues**: Check that all msgspec structs are properly defined

### Performance Considerations

- **Decorated runnable** has the best performance (pre-registered, no overhead)
- **Named runnable** has excellent performance (cached imports, no serialization overhead)  
- **UDF** has compilation overhead but is very flexible

## Next Steps

To extend this example:

1. **Add more LangChain integrations**: Try different runnable types (chains, agents, etc.)
2. **Custom components**: Create domain-specific business logic components
3. **Streaming support**: Implement streaming for long-running operations
4. **Error handling**: Add comprehensive error handling and recovery
5. **Performance optimization**: Leverage import caching, optimize hot paths