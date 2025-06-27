# OpenAI Component Example

This example demonstrates how to use the OpenAI built-in component with Stepflow.

## Prerequisites

1. An OpenAI API key
2. The Stepflow CLI built from this repository

## Setup

Set your OpenAI API key as an environment variable:

```bash
export OPENAI_API_KEY=your_api_key_here
```

## Running the Example

From the repository root, run:

```bash
cargo run -- run --flow=examples/openai/openai_chat.yaml --input=examples/openai/input.json --config=examples/openai/stepflow-config.yml
```

## Example Flow

The example flow (`openai_chat.yaml`) demonstrates:

1. Calling the OpenAI chat completion API
2. Passing system and user messages
3. Extracting the response from the API

## Additional Configuration

The OpenAI component supports several configuration options:

- `messages`: An array of messages to send to the API (required)
- `max_tokens`: Maximum number of tokens to generate (optional)
- `temperature`: Controls randomness of responses (optional, between 0 and 2)
- `api_key`: Override the API key from environment (optional)

## Available Components

- `builtins:/openai/chat` - Uses the `gpt-3.5-turbo` model
- `builtins:/openai/chat/gpt4` - Uses the `gpt-4` model

## Example Input

```json
{
  "prompt": "Explain quantum computing in simple terms.",
  "system_message": "You are a helpful expert explaining complex topics to beginners."
}
```

## Example Output

```json
{
  "response": "Quantum computing is like...",
  "message": {
    "role": "assistant",
    "content": "Quantum computing is like..."
  }
}
```