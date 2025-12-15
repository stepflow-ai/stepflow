---
sidebar_position: 2
---

# Getting Started

This guide will help you install Stepflow and run your first workflow in just a few minutes.

Stepflow is a **workflow orchestrator** that coordinates the execution of components across different servers. In this tutorial, you'll see how the Stepflow runtime orchestrates both built-in components and a custom Python component server to create a complete workflow.

## Setup

### 1. Download Stepflow

Download the latest pre-built binary from [GitHub Releases](https://github.com/stepflow-ai/stepflow/releases) for your platform.

### 2. Install Python SDK (Recommended)

Most workflows use custom Python components for data processing and integrations:

```bash
# Install Python 3.11+ and uv (Python package manager)
pip install uv

# Install the Python SDK
uv pip install stepflow_py
```

:::note
The Python SDK is technically optional if you plan to use only built-in components or other language SDKs.
See [Components](./components/index.md) for alternatives.
:::


## Your First Workflow

Let's create a simple workflow that combines built-in components with a custom Python function.

### 1. Set up OpenAI API

This example uses OpenAI's GPT models via the built-in OpenAI component.
You'll need to configure an OpenAI API key for this, as shown below.

```bash
export OPENAI_API_KEY="your-api-key-here"
```

### 2. Create `hello-workflow.yaml`

```yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "Hello Stepflow"
description: "A simple workflow combining built-in and Python components"

input_schema:
  type: object
  properties:
    name:
      type: string
      description: "User's name"
  required: [name]

steps:
  # Step 1: Use Python component to format greeting
  - id: format_greeting
    component: /python/hello_formatter
    input:
      name: { $from: { workflow: input }, path: "name" }

  # Step 2: Use built-in component to create AI messages
  - id: create_messages
    component: /builtin/create_messages
    input:
      system_instructions: "You are a friendly assistant. Respond enthusiastically to greetings."
      user_prompt: { $from: { step: format_greeting }, path: "greeting" }

  # Step 3: Generate AI response
  - id: ai_response
    component: /builtin/openai
    input:
      messages: { $from: { step: create_messages }, path: "messages" }
      temperature: 0.7
      max_tokens: 100

output:
  greeting: { $from: { step: format_greeting }, path: "greeting" }
  ai_response: { $from: { step: ai_response }, path: "response" }
```

### 3. Create `hello_formatter.py` (Python Component)

```python
from stepflow_py import StepflowStdioServer
import msgspec

class Input(msgspec.Struct):
    name: str

class Output(msgspec.Struct):
    greeting: str

server = StepflowStdioServer()

@server.component
def hello_formatter(input: Input) -> Output:
    greeting = f"Hello, {input.name}! Welcome to Stepflow."
    return Output(greeting=greeting)

if __name__ == "__main__":
    server.run()
```


### 4. Create `stepflow-config.yml` (Configuration)

```yaml
plugins:
  builtin:
    type: builtin
  python:
    type: stepflow
    transport: stdio
    command: uv
    args: ["run", "python", "hello_formatter.py"]

routes:
  "/python/{*component}":
    - plugin: python
  "/builtin/{*component}":
    - plugin: builtin
```

### 5. Create `input.json`

```json
{
  "name": "World"
}
```

### 6. Run the Workflow

```bash
stepflow run --flow=hello-workflow.yaml --input=input.json
```

You should see output like:

```json
{
  "greeting": "Hello, World! Welcome to Stepflow.",
  "ai_response": "Hello, World! Welcome to Stepflow! 🎉 It's wonderful to meet you..."
}
```

## What Just Happened?

This workflow demonstrates Stepflow's key concepts as a **workflow orchestrator**:

- **Orchestration**: Stepflow coordinated the execution of three different components, managing data flow and dependencies between them
- **Component Servers**: The workflow used both built-in components (managed by Stepflow) and a custom Python component server (launched as a subprocess)
- **Input/Output Schemas**: Define the structure of your data using JSON Schema
- **Steps**: Each step uses a component to process data and pass results to the next step
- **Data Flow**: Use `$from` expressions to pass data between steps, with Stepflow handling the routing and transformation
- **Configuration**: The `stepflow-config.yml` file told Stepflow how to route component requests to the appropriate servers

## Next Steps

Choose your learning path based on your goals:

- **Build workflows**: Learn about [Flows](./flows/index.md) - steps, expressions, and control flow
- **Use and create components**: Explore [Components](./components/index.md) - built-ins, Python SDK, and other language SDKs
- **Test workflows**: Check out [Testing Best Practices](./best-practices/testing.md) for validation and testing strategies
- **See more examples**: Browse [Workflow Examples](./examples/) for real-world use cases