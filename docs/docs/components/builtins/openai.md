---
sidebar_position: 7
---

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

**Note**: The role field uses camelCase serialization internally, but accepts both formats.

#### Example

```yaml
steps:
  - id: ask_ai
    component: /builtin/openai
    input:
      messages: { $step: prepare_chat, path: "messages" }
      max_tokens: 200
      temperature: 0.3
```

#### Complete AI Workflow Example

```yaml
steps:
  - id: create_prompt
    component: /builtin/create_messages
    input:
      system_instructions: "You are a code review assistant. Analyze code for potential issues."
      user_prompt: { $input: "code_snippet" }

  - id: analyze_code
    component: /builtin/openai
    input:
      messages: { $step: create_prompt, path: "messages" }
      max_tokens: 500
      temperature: 0.2
```