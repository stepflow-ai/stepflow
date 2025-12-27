---
sidebar_position: 6
---

# Secrets

Stepflow provides built-in secret redaction to protect sensitive data in logs, error messages, and debug output. By marking fields as secrets in your schemas, Stepflow automatically redacts these values throughout the workflow execution.

## Overview

Secret redaction works by:
1. **Schema Annotation**: Mark fields as `is_secret: true` in JSON schemas
2. **Automatic Detection**: Stepflow analyzes schemas to identify secret fields
3. **Runtime Redaction**: Secret values are replaced with `[REDACTED]` in all output
4. **Nested Support**: Works with nested objects and arrays

## Flow Input Secrets

Mark sensitive workflow inputs as secrets in the input schema:

```yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "Secure API Workflow"
description: "Workflow that handles sensitive API credentials"

schemas:
  type: object
  properties:
    input:
      type: object
      properties:
        user_id:
          type: string
          description: "User identifier"
        api_credentials:
          type: object
          properties:
            api_key:
              type: string
              is_secret: true
              description: "API key for external service"
            secret_token:
              type: string
              is_secret: true
              description: "Authentication token"
            endpoint:
              type: string
              description: "API endpoint URL"
          required: ["api_key", "secret_token", "endpoint"]
      required: ["user_id", "api_credentials"]

steps:
  - id: call_api
    component: /http/request
    input:
      url: { $input: "api_credentials.endpoint" }
      headers:
        Authorization:
          { $input: "$" }
          path: "api_credentials.api_key"
          transform: "Bearer " + x
```

## Component Input Secrets

Custom components can also define secret fields in their input schemas:

```python
# Python component with secret input
from stepflow_py import StepflowStdioServer
import msgspec

class DatabaseConfig(msgspec.Struct):
    host: str
    port: int = 5432
    username: str
    password: str  # Will be marked as secret in schema
    database: str

class Input(msgspec.Struct):
    query: str
    config: DatabaseConfig

class Output(msgspec.Struct):
    results: list[dict]
    row_count: int

server = StepflowStdioServer()

@server.component
def execute_query(input: Input) -> Output:
    # Component implementation
    # The password field will be automatically redacted in logs
    pass

# Define the component schema with secret annotation
server.register_component_schema("execute_query", {
    "input": {
        "type": "object",
        "properties": {
            "query": {"type": "string"},
            "config": {
                "type": "object",
                "properties": {
                    "host": {"type": "string"},
                    "port": {"type": "integer", "default": 5432},
                    "username": {"type": "string"},
                    "password": {
                        "type": "string",
                        "is_secret": true,
                        "description": "Database password"
                    },
                    "database": {"type": "string"}
                },
                "required": ["host", "username", "password", "database"]
            }
        },
        "required": ["query", "config"]
    }
})
```

## Variable Secrets

Mark variables as secrets in the variables schema:

```yaml
# variables-schema.yaml
type: object
properties:
  openai_api_key:
    type: string
    is_secret: true
    description: "OpenAI API key"
  database_url:
    type: string
    is_secret: true
    description: "Database connection string"
  debug_mode:
    type: boolean
    default: false
    description: "Enable debug logging"
required: ["openai_api_key", "database_url"]
```

```yaml
# workflow.yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "Workflow with Secret Variables"
variables_schema: "./variables-schema.yaml"

steps:
  - id: setup_ai
    component: /builtin/openai
    input:
      api_key: { $variable: openai_api_key }
      model: "gpt-4"
      messages: [{"role": "user", "content": "Hello"}]
```

## Nested Secrets

Secrets work with nested objects and arrays:

```yaml
schemas:
  type: object
  properties:
    input:
      type: object
      properties:
        services:
          type: array
          items:
            type: object
            properties:
              name:
                type: string
              credentials:
                type: object
                properties:
                  username:
                    type: string
                  password:
                    type: string
                    is_secret: true
                  api_key:
                    type: string
                    is_secret: true
                required: ["username", "password"]
            required: ["name", "credentials"]
```

With this schema, input like:
```json
{
  "services": [
    {
      "name": "service1",
      "credentials": {
        "username": "user1",
        "password": "secret123",
        "api_key": "sk-abc123"
      }
    }
  ]
}
```

Will be logged as:
```json
{
  "services": [
    {
      "name": "service1",
      "credentials": {
        "username": "user1",
        "password": "[REDACTED]",
        "api_key": "[REDACTED]"
      }
    }
  ]
}
```

## Secret Arrays

Mark entire arrays as secret:

```yaml
schemas:
  type: object
  properties:
    input:
      type: object
      properties:
        sensitive_data:
          type: array
          is_secret: true
          items:
            type: object
            properties:
              id: { type: string }
              value: { type: string }
```

The entire array will be redacted as `[REDACTED]` in logs.

## Security Considerations

1. **Log Safety**: Secret redaction prevents accidental exposure in application logs
2. **Error Messages**: Secrets are redacted in error messages and stack traces
3. **Debug Output**: Development and debugging output automatically redacts secrets
4. **Component Isolation**: Each component server handles its own secret redaction
5. **Transport Security**: Use HTTPS/TLS for component server communication in production

## Limitations

- Secret redaction is for **display purposes only** - the actual values are still processed by components
- Redaction occurs at the Stepflow runtime level, not within component implementations
- Components must implement their own security measures for handling secret data
- Secret fields should still be transmitted securely (HTTPS, encrypted storage, etc.)

## Related Documentation

- [Input and Output](./input-output.md) - Flow input/output schemas
- [Configuration](../configuration.md) - Environment variable management
- [Best Practices](../best-practices/design.md) - Security best practices