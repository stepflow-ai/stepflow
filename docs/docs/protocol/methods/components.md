---
sidebar_position: 3
---

import SchemaDisplay from "@site/src/components/SchemaDisplay";

# Components

Component methods enable the discovery, introspection, and execution of workflow components. These methods form the core of the Stepflow Protocol and are used extensively during workflow execution.

## Overview

The component methods provide a complete lifecycle for working with components:

1. **`components/list`** - Discover all available components
2. **`components/info`** - Get detailed information about a specific component
3. **`components/execute`** - Execute a component with input data

## components/list Method

**Method Name:** `components/list`
**Direction:** Runtime → Component Server
**Type:** Request (expects response)

<SchemaDisplay schema="https://stepflow.org/schemas/v1/protocol.json" path="$defs/ComponentListParams"/>

<SchemaDisplay schema="https://stepflow.org/schemas/v1/protocol.json" path="$defs/ListComponentsResult"/>

### Request Example

```json
{
  "jsonrpc": "2.0",
  "id": "list-components-001",
  "method": "components/list",
  "params": {}
}
```

### Response Example

```json
{
  "jsonrpc": "2.0",
  "id": "list-components-001",
  "result": {
    "components": [
      {
        "name": "data_processor",
        "description": "Process and transform data records according to configurable rules"
      },
      {
        "name": "http_client",
        "description": "Make HTTP requests with automatic retry logic and response parsing"
      }
    ]
  }
}
```

## components/info Method

**Method Name:** `components/info`
**Direction:** Runtime → Component Server
**Type:** Request (expects response)

<SchemaDisplay schema="https://stepflow.org/schemas/v1/protocol.json" path="$defs/ComponentInfoParams"/>

<SchemaDisplay schema="https://stepflow.org/schemas/v1/protocol.json" path="$defs/ComponentInfoResult"/>

### Request Example

```json
{
  "jsonrpc": "2.0",
  "id": "component-info-001",
  "method": "components/info",
  "params": {
    "component": {
      "name": "data_processor",
      "path": "/python/data_processor"
    }
  }
}
```

### Response Example

```json
{
  "jsonrpc": "2.0",
  "id": "component-info-001",
  "result": {
    "info": {
      "name": "data_processor",
      "description": "Process and transform data records according to configurable rules",
      "input_schema": {
        "type": "object",
        "properties": {
          "records": {
            "type": "array",
            "items": {"type": "object"}
          },
          "rules": {
            "type": "object",
            "properties": {
              "transformation": {
                "type": "string",
                "enum": ["uppercase", "lowercase", "title_case"]
              }
            }
          }
        },
        "required": ["records", "rules"]
      },
      "output_schema": {
        "type": "object",
        "properties": {
          "processed_records": {"type": "array"},
          "summary": {"type": "object"}
        },
        "required": ["processed_records", "summary"]
      }
    }
  }
}
```

## components/execute Method

**Method Name:** `components/execute`
**Direction:** Runtime → Component Server
**Type:** Request (expects response)

<SchemaDisplay schema="https://stepflow.org/schemas/v1/protocol.json" path="$defs/ComponentExecuteParams"/>

<SchemaDisplay schema="https://stepflow.org/schemas/v1/protocol.json" path="$defs/ComponentExecuteResult"/>

### Request Example

```json
{
  "jsonrpc": "2.0",
  "id": "execute-data-processor-001",
  "method": "components/execute",
  "params": {
    "component": {
      "name": "data_processor",
      "path": "/python/data_processor"
    },
    "input": {
      "records": [
        {"id": "record_1", "data": {"name": "John", "status": "active"}}
      ],
      "rules": {
        "transformation": "uppercase"
      }
    }
  }
}
```

### Response Example

```json
{
  "jsonrpc": "2.0",
  "id": "execute-data-processor-001",
  "result": {
    "output": {
      "processed_records": [
        {
          "id": "record_1",
          "data": {"name": "JOHN", "status": "ACTIVE"},
          "processed": true
        }
      ],
      "summary": {
        "total": 1,
        "processed": 1,
        "errors": 0
      }
    }
  }
}
```

### The `attempt` Field

The `components/execute` request includes an `attempt` field -- a **1-based, monotonically increasing counter** that tracks how many times a step has been executed. On the first execution, `attempt` is `1`. Each subsequent re-execution increments the counter by one.

The `attempt` counter is shared across all retry reasons. A step may be re-executed for any of the following reasons:

| Reason | Trigger | Limit | Configured Via |
|--------|---------|-------|----------------|
| **Transport error** | Subprocess crash, network timeout, connection failure | `transportMaxRetries` (default: 3) | Orchestrator `retry` config |
| **Component error** | Component ran and returned an error; step has `onError: { action: retry }` | `maxRetries` (default: 3) | Step-level `onError` |
| **Orchestrator recovery** | Orchestrator crashed; task was started but no completion journaled | Unlimited | N/A |

Transport errors and component errors have **separate budgets** -- exhausting transport retries does not consume component retry budget, and vice versa. The `attempt` counter increments regardless of which retry reason triggered the re-execution.

```json title="First execution"
{
  "jsonrpc": "2.0",
  "id": "execute-001",
  "method": "components/execute",
  "params": {
    "component": { "name": "data_processor", "path": "/python/data_processor" },
    "input": { "records": [] },
    "attempt": 1
  }
}
```

```json title="Third execution (after two retries)"
{
  "jsonrpc": "2.0",
  "id": "execute-003",
  "method": "components/execute",
  "params": {
    "component": { "name": "data_processor", "path": "/python/data_processor" },
    "input": { "records": [] },
    "attempt": 3
  }
}
```

Components can use the `attempt` field for observability (logging, metrics) or to adjust their behavior on retries (e.g., using a longer timeout or a different strategy).

### Bidirectional Execution

Components that need to interact with the runtime during execution can make requests back to the runtime:

```mermaid
sequenceDiagram
    participant R as Runtime
    participant S as Component Server

    R->>+S: components/execute request
    Note over S: Component execution in progress

    S->>+R: blobs/put (store data)
    R-->>-S: blob_id response

    Note over S: Continue processing

    S-->>-R: execution result
```