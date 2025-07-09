---
sidebar_position: 2
---

# Getting Started

This guide will help you install StepFlow and run your first workflow.

## Installation

### Prerequisites

- **Rust** (only if building from source)

### Pre-built Binaries (Recommended)

Download the latest release from [GitHub Releases](https://github.com/riptano/stepflow/releases).

### Building from Source

If you prefer to build from source or need the latest development version:

```bash
git clone https://github.com/riptano/stepflow.git
cd stepflow/stepflow-rs
cargo build --release
```

The binary will be available at `target/release/stepflow`.

### Optional: Python SDK

If you want to create custom components using Python, you'll need:

- **Python 3.8+**
- **uv** (Python package manager)

The Python SDK is not required for basic StepFlow usage.

## Your First Workflow

Let's create a simple AI-powered question answering application that demonstrates how StepFlow can orchestrate AI workflows using simple schema definitions and builtin components.

### 1. Create a Workflow File

From the `stepflow` project directory, create a file called `ai_qa_workflow.yaml`:

```yaml
name: "AI Question Answering"
description: "Demonstrates a simple AI workflow with context and generation"

input_schema:
  type: object
  properties:
    question:
      type: string
      description: "User's question"
    context:
      type: string
      description: "Context information to help answer the question"
  required: [question, context]

output_schema:
  type: object
  properties:
    answer:
      type: string
      description: "AI-generated answer"
    metadata:
      type: object
      description: "Processing metadata"

steps:
  # Store the context as a blob for potential reuse
  - id: store_context
    component: builtin://put_blob
    input:
      data:
        context: { $from: { workflow: input }, path: "context" }
        question: { $from: { workflow: input }, path: "question" }

    # Format the context and question into a clear prompt
  - id: format_prompt
    component: builtin://put_blob
    input:
      data:
        context: { $from: { workflow: input }, path: "context" }
        question: { $from: { workflow: input }, path: "question" }

  # Get the formatted prompt from the blob
  - id: get_prompt
    component: builtin://get_blob
    input:
      blob_id: { $from: { step: format_prompt }, path: "blob_id" }

  # Create messages for OpenAI
  - id: create_messages
    component: builtin://create_messages
    input:
      system_instructions: "You are a helpful assistant. Answer the user's question based on the provided context. If the context doesn't contain enough information, say so and provide your best general answer."
      user_prompt: { $from: { workflow: input }, path: "question" }

  # Generate AI response using OpenAI  
  - id: generate_answer
    component: builtin://openai
    input:
      messages: { $from: { step: create_messages }, path: "messages" }
      temperature: 0.3
      max_tokens: 150

  # Create response metadata
  - id: create_metadata
    component: builtin://put_blob
    input:
      data:
        processed_at: "2024-01-15T10:30:00Z"
        model_used: "gpt-3.5-turbo"
        workflow_version: "1.0"
        question_length: 25
        context_provided: true

output:
  answer: { $from: { step: generate_answer }, path: "response" }
  metadata: { $from: { step: create_metadata } }
```

### 2. Create an Input File

Similarly, create `input.json` with the `question` and `context` properties defined in the `input_schema` of the YAML file:

```json
{
  "question": "What is the capital of France?",
  "context": "Paris is the capital and largest city of France, located in northern France on the river Seine. It's known for landmarks like the Eiffel Tower and Louvre Museum."
}
```

### 3. Set Up OpenAI API

You'll need an OpenAI API key. Set it as an environment variable:

```bash
export OPENAI_API_KEY="your-api-key-here"
```

### 4. Run the Workflow

Now we call `cargo run -- run` from the `stepflow-rs` directory with the path to the workflow and input file:

```bash
cd stepflow-rs
cargo run -- run --flow=ai_qa_workflow.yaml --input=input.json
```

You should see output with the `answer` and `metadata` properties as defined in the `output_schema` of the YAML file:

```json
{
  "answer": "Based on the provided context, the capital of France is Paris. It is the largest city in France and is located in northern France on the river Seine. Paris is famous for its landmarks including the Eiffel Tower and the Louvre Museum.",
  "metadata": {
    "blob_id": "sha256:a1b2c3d4..."
  }
}
```

### 5. Understanding the Workflow

This AI Q&A workflow demonstrates several key StepFlow concepts:

- **Input/Output Schemas**: Define the structure of questions, context, and answers
- **AI Integration**: Use [`builtin://openai`](./components/builtins.md#builtinopenai) and [`builtin://create_messages`](./components/builtins.md#builtincreate_messages) for natural language processing
- **Data References**: Use [`$from`](./workflows/expressions.md#data-references-from) to pass data between steps
- **Blob Storage**: Store context and metadata for reuse with [`builtin://put_blob`](./components/builtins.md#builtinput_blob)
- **Multi-step Processing**: Orchestrate data storage, prompt formatting, and AI generation

The workflow follows this pattern:
1. **Store Context**: Input data is stored as blobs for efficient reuse
2. **Format Prompt**: Context and question are formatted into a clear prompt
3. **Create Messages**: System instructions and user prompts are structured for OpenAI
4. **AI Generation**: OpenAI generates answers based on the provided context
5. **Metadata Tracking**: Processing information is captured for observability

## Alternative Input Methods

You can provide input in several ways:

```bash
# Using inline JSON
cargo run -- run --flow=ai_qa_workflow.yaml --input-json='{"question": "What is AI?", "context": "Artificial Intelligence is the simulation of human intelligence in machines..."}'

# Using stdin
echo '{"question": "What is machine learning?", "context": "Machine learning is..."}' | cargo run -- run --flow=ai_qa_workflow.yaml --stdin-format=json
```

## Testing Your Workflow

You can add test cases directly to your workflow file:

```yaml
test:
  cases:
  - name: basic_qa_test
    description: Test AI Q&A with a simple geography question
    input:
      question: "What is the capital of France?"
      context: "Paris is the capital and largest city of France, located in northern France on the river Seine."
    output:
      outcome: success
      result:
        answer: "*Paris*"  # Answer should contain "Paris"
        metadata:
          blob_id: "sha256:*"
```

Run tests with:

```bash
cargo run -- test ai_qa_workflow.yaml
```

For more advanced testing patterns and configuration options, see the [Testing Guide](./workflows/testing.md).

## Next Steps

Now that you've run your first RAG workflow, explore these topics to build more sophisticated applications:

### Workflow Development
* **[Workflow Fundamentals](./workflows/steps.md)** - Understanding steps, execution order, and dependencies
* **[Input & Output](./workflows/input_output.md)** - Data flow patterns and schema validation  
* **[Expression System](./workflows/expressions.md)** - Advanced data references and transformations
* **[Testing](./workflows/testing.md)** - Comprehensive testing patterns and CI/CD integration
* **[Workflow Specification](./workflows/specification.md)** - Complete workflow file format reference

### Components & Integration
* **[Builtin Components](./components/builtins.md)** - Complete reference for OpenAI, blob storage, and other builtins
* **[Custom Components](./components/index.md)** - Build your own components in Python, TypeScript, or Rust
* **[Protocol Documentation](./protocol/components.mdx)** - Understanding the component communication protocol

### Runtime & Deployment
* **[CLI Reference](./runtime/commands.md)** - Complete command-line interface documentation
* **[Configuration](./runtime/config.md)** - Plugin setup, state stores, and environment configuration
* **[Runtime Options](./runtime/index.md)** - Local execution, service mode, and deployment patterns

### More Examples

You've just built a simple AI Q&A application! Check out the `examples/` directory in the repository for more sophisticated workflows:

- **Data Pipeline** (`examples/data-pipeline/`) - Multi-step data processing with AI insights
- **Advanced RAG** (`examples/openai/`) - Full retrieval-augmented generation with document search
- **Custom Components** (`examples/custom-python-components/`) - Build your own components (requires Python SDK)

### AI-Free Workflows

The example above requires an OpenAI API key. For workflows that don't require AI, you can use:
- Data transformation workflows with just `builtin://put_blob`
- HTTP API integrations with `builtin://http_request`
- File processing and validation workflows

### Key Concepts to Explore

- **Parallel Execution**: Steps run in parallel when possible
- **Conditional Logic**: Skip steps based on conditions
- **Error Handling**: Graceful failure handling with fallbacks
- **Nested Workflows**: Execute sub-workflows within steps
- **Component Servers**: Create reusable components in any language