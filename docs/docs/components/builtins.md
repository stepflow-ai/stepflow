---
sidebar_position: 2
---

# Builtin Components

StepFlow provides several built-in components that handle common workflow operations. These components are available without any additional configuration and cover data storage, file operations, AI integration, and workflow composition.

## Data Storage Components

### `put_blob`

Store JSON data as content-addressable blobs for efficient reuse across workflow steps.

#### Input

```yaml
input:
  data: <any JSON data>
```

- **`data`** (required): Any JSON data to store as a blob

#### Output

```yaml
output:
  blob_id: "sha256:abc123..."
```

- **`blob_id`**: SHA-256 hash of the stored data

#### Example

```yaml
steps:
  - id: store_user_data
    component: put_blob
    input:
      data:
        user_id: { $from: { workflow: input }, path: "user_id" }
        profile: { $from: { step: load_profile } }
        preferences: { $from: { step: load_preferences } }
```

### `get_blob`

Retrieve JSON data from previously stored blobs.

#### Input

```yaml
input:
  blob_id: "sha256:abc123..."
```

- **`blob_id`** (required): Content hash of the blob to retrieve

#### Output

```yaml
output:
  data: <stored JSON data>
```

- **`data`**: The original JSON data that was stored

#### Example

```yaml
steps:
  - id: retrieve_user_data
    component: get_blob
    input:
      blob_id: { $from: { step: store_user_data }, path: "blob_id" }
```

#### Use Cases

- **Large Data Reuse**: Store large datasets once and reference them multiple times
- **User-Defined Functions**: Store Python code and reuse across steps
- **Configuration Storage**: Store complex configuration objects
- **Intermediate Results**: Cache expensive computation results

## File Operations

### `load_file`

Load and parse files from the filesystem with automatic format detection.

#### Input

```yaml
input:
  path: "path/to/file.json"
  format: "json"  # optional: "json", "yaml", "text"
```

- **`path`** (required): File path to load
- **`format`** (optional): Force specific format, otherwise auto-detected from extension

#### Output

```yaml
output:
  data: <parsed file content>
  metadata:
    resolved_path: "/absolute/path/to/file.json"
    size_bytes: 1024
    format: "json"
```

- **`data`**: Parsed file content (JSON object, YAML object, or text string)
- **`metadata`**: File information

#### Supported Formats

- **JSON** (`.json`): Parsed as JSON object
- **YAML** (`.yaml`, `.yml`): Parsed as YAML object
- **Text** (other extensions): Loaded as raw string

#### Example

```yaml
steps:
  - id: load_config
    component: load_file
    input:
      path: "config/settings.yaml"

  - id: load_data
    component: load_file
    input:
      path: { $from: { workflow: input }, path: "data_file" }
      format: "json"
```

## AI Integration Components

### `create_messages`

Create structured chat message arrays for AI models from system instructions and user prompts.

#### Input

```yaml
input:
  system_instructions: "You are a helpful assistant"  # optional
  user_prompt: "What is the capital of France?"        # required
```

- **`user_prompt`** (required): The user's question or prompt
- **`system_instructions`** (optional): System-level instructions for the AI

#### Output

```yaml
output:
  messages:
    - role: "system"
      content: "You are a helpful assistant"
    - role: "user"
      content: "What is the capital of France?"
```

- **`messages`**: Array of chat messages with roles and content

#### Example

```yaml
steps:
  - id: prepare_chat
    component: create_messages
    input:
      system_instructions: "You are an expert data analyst"
      user_prompt: { $from: { workflow: input }, path: "question" }
```

### `openai`

Send messages to OpenAI's chat completion API and receive responses.

#### Input

```yaml
input:
  messages:
    - role: "system"
      content: "You are a helpful assistant"
    - role: "user"
      content: "Hello!"
  max_tokens: 150      # optional
  temperature: 0.7     # optional
  api_key: "sk-..."    # optional (uses OPENAI_API_KEY env var)
```

- **`messages`** (required): Array of chat messages
- **`max_tokens`** (optional): Maximum tokens in response
- **`temperature`** (optional): Response randomness (0.0 to 1.0)
- **`api_key`** (optional): OpenAI API key (uses environment variable if not provided)

#### Output

```yaml
output:
  response: "Hello! How can I help you today?"
```

- **`response`**: The AI's text response

#### Message Format

Each message must have:
- **`role`**: "system", "user", or "assistant"
- **`content`**: Text content of the message

#### Example

```yaml
steps:
  - id: ask_ai
    component: openai
    input:
      messages: { $from: { step: prepare_chat }, path: "messages" }
      max_tokens: 200
      temperature: 0.3
```

#### Complete AI Workflow Example

```yaml
steps:
  - id: create_prompt
    component: create_messages
    input:
      system_instructions: "You are a code review assistant. Analyze code for potential issues."
      user_prompt: { $from: { workflow: input }, path: "code_snippet" }

  - id: analyze_code
    component: openai
    input:
      messages: { $from: { step: create_prompt }, path: "messages" }
      max_tokens: 500
      temperature: 0.2
```

## Workflow Composition

### `eval`

Execute nested workflows with isolated execution contexts.

#### Input

```yaml
input:
  workflow:
    inputs: { }
    steps: [ ]
    output: { }
  input: <data for nested workflow>
```

- **`workflow`** (required): Complete workflow definition to execute
- **`input`** (required): Input data for the nested workflow

#### Output

```yaml
output:
  result: <output from nested workflow>
  run_id: "uuid-of-execution"
```

- **`result`**: The output produced by the nested workflow
- **`run_id`**: Unique identifier for the nested execution

#### Example

```yaml
steps:
  - id: run_analysis
    component: eval
    input:
      workflow:
        input_schema:
          type: object
          properties:
            data: { type: array }
        steps:
          - id: process
            component: data/analyze
            input:
              data: { $from: { workflow: input }, path: "data" }
          - id: summarize
            component: data/summarize
            input:
              analysis: { $from: { step: process } }
        output:
          summary: { $from: { step: summarize } }
      input:
        data: { $from: { step: load_data } }
```

#### Use Cases

- **Workflow Modularity**: Break complex workflows into reusable sub-workflows
- **Dynamic Execution**: Execute different workflows based on runtime conditions
- **Parallel Sub-workflows**: Run multiple independent workflows concurrently
- **Testing**: Execute workflows in isolation for testing purposes