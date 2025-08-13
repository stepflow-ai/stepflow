---
sidebar_position: 5
---

import SchemaDisplay from "@site/src/components/SchemaDisplay";

# Flows

Flow methods enable component servers to evaluate workflow expressions and access runtime context during execution. These methods provide advanced integration capabilities for components that need to understand their execution environment.

## Overview

The flow methods currently include:

1. **`flows/evaluate`** - Evaluate workflow expressions in runtime context

Additional flow methods may be added in future protocol versions for workflow introspection and runtime interaction.

## Flow Evaluation Sequence

```mermaid
sequenceDiagram
    participant R as Runtime
    participant S as Component Server

    Note over R,S: Component execution in progress

    S->>+R: flows/evaluate (expression)
    Note over R: Parse expression syntax
    Note over R: Resolve references in current context
    R-->>-S: evaluated result

    Note over S: Use resolved value in component logic
```

## flows/evaluate Method

**Method Name:** `flows/evaluate`  
**Direction:** Component Server → Runtime  
**Type:** Request (expects response)

<SchemaDisplay schema="https://stepflow.org/schemas/v1/protocol.json" path="$defs/EvaluateFlowParams"/>

<SchemaDisplay schema="https://stepflow.org/schemas/v1/protocol.json" path="$defs/EvaluateFlowResult"/>

### Request Example

```json
{
  "jsonrpc": "2.0",
  "id": "eval-flow-001",
  "method": "flows/evaluate",
  "params": {
    "expression": {
      "$from": {"step": "data_processor"},
      "path": "$.results.total_count"
    }
  }
}
```

### Response Example

```json
{
  "jsonrpc": "2.0",
  "id": "eval-flow-001",
  "result": {
    "value": 42
  }
}
```

## Expression Types

### Workflow Input References

```json
{
  "expression": {
    "$from": {"workflow": "input"},
    "path": "$.user.preferences.theme"
  }
}
```

### Step Output References

```json
{
  "expression": {
    "$from": {"step": "user_validation"},
    "path": "$.validation_result.is_valid"
  }
}
```

### Complex Expressions

```json
{
  "expression": {
    "user_data": {
      "$from": {"workflow": "input"},
      "path": "$.user"
    },
    "validation_status": {
      "$from": {"step": "user_validation"},
      "path": "$.status"
    },
    "static_config": {
      "timeout": 30
    }
  }
}
```

