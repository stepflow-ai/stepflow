---
sidebar_position: 6
---

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
    component: /builtin/create_messages
    input:
      system_instructions: "You are an expert data analyst"
      user_prompt: { $input: "question" }
```