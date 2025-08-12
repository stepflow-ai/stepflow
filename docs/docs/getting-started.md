---
sidebar_position: 2
---

# Getting Started

This guide will help you install StepFlow and run your first workflow in just a few minutes.

## Setup

### 1. Download StepFlow

Download the latest pre-built binary from [GitHub Releases](https://github.com/riptano/stepflow/releases) for your platform.

### 2. Install Python SDK (Recommended)

Most workflows use custom Python components for data processing and integrations:

```bash
# Install Python 3.11+ and uv (Python package manager)
pip install uv

# The Python SDK will be automatically available when needed
```

:::note
The Python SDK is technically optional if you plan to use only built-in components or other language SDKs.
See [Components](./components/index.md) for alternatives.
:::

### 3. Set up OpenAI API

This example uses OpenAI's GPT models via the built-in OpenAI component.
You'll need to configure an OpenAI API key for this, as shown below.

```bash
export OPENAI_API_KEY="your-api-key-here"
```

## Your First Workflow

Let's create a simple workflow that combines built-in components with a custom Python function.

### 1. Create `hello-workflow.yaml`

```yaml
name: "Hello StepFlow"
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

### 2. Create `hello_formatter.py` (Python Component)

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
    greeting = f"Hello, {input.name}! Welcome to StepFlow."
    return Output(greeting=greeting)

if __name__ == "__main__":
    server.run()
```


### 3. Create `stepflow-config.yml` (Configuration)

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

### 4. Create `input.json`

```json
{
  "name": "World"
}
```

### 5. Run the Workflow

```bash
stepflow run --flow=hello-workflow.yaml --input=input.json
```

You should see output like:

```json
{
  "greeting": "Hello, World! Welcome to StepFlow.",
  "ai_response": "Hello, World! Welcome to StepFlow! 🎉 It's wonderful to meet you..."
}
```

## What Just Happened?

This workflow demonstrates StepFlow's key concepts:

- **Input/Output Schemas**: Define the structure of your data using JSON Schema
- **Steps**: Each step uses a component to process data and pass results to the next step
- **Components**: Built-in components (`/builtin/openai`, `/builtin/create_messages`) and custom Python components (`/python/hello_formatter`)
- **Data Flow**: Use `$from` expressions to pass data between steps

## Next Steps

Choose your learning path based on your goals:

- **Build workflows**: Learn about [Flows](./flows/index.md) - steps, expressions, and control flow
- **Create components**: Explore [Components](./components/index.md) - built-ins, Python SDK, and other language SDKs
- **Test workflows**: Check out [Testing Best Practices](./best-practices/testing.md) for validation and testing strategies
- **See examples**: Browse [Workflow Examples](./examples/) for real-world use cases