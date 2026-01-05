---
sidebar_position: 8
---

# Batch Execution

Batch execution allows you to run the same workflow on multiple inputs in parallel, maximizing throughput and resource utilization. Stepflow provides both local and remote batch execution with configurable concurrency and progress tracking.

## Overview

Batch execution enables:
- **Parallel Processing**: Execute multiple workflow instances simultaneously
- **Concurrency Control**: Limit concurrent executions to manage resources
- **Progress Tracking**: Monitor batch completion and individual run status
- **Efficient Resource Usage**: Reuse component servers across batch items
- **Fault Isolation**: Individual failures don't affect other batch items

## Local Batch Execution

Execute batches locally using the `run` command with the `--inputs` flag:

```bash
stepflow run --flow=workflow.yaml --inputs=batch-inputs.jsonl
```

### Input Format

Batch inputs are provided as JSONL (JSON Lines) format - one JSON object per line:

```jsonl
{"user_id": "user1", "action": "process"}
{"user_id": "user2", "action": "analyze"}
{"user_id": "user3", "action": "process"}
```

### Basic Example

```bash
# Process 100 items with default concurrency
stepflow run \
  --flow=data-pipeline.yaml \
  --inputs=items.jsonl \
  --output=results.jsonl
```

### Concurrency Control

Limit concurrent executions to manage resource usage:

```bash
# Limit to 10 concurrent executions
stepflow run \
  --flow=workflow.yaml \
  --inputs=batch-inputs.jsonl \
  --max-concurrent=10
```

**Choosing Concurrency:**
- **CPU-bound workflows**: Set to number of CPU cores
- **I/O-bound workflows**: Set higher (2-4x CPU cores)
- **Memory-intensive workflows**: Set lower to avoid OOM
- **External API calls**: Respect rate limits

## Remote Batch Execution

Submit batches to a remote Stepflow server for distributed execution:

```bash
stepflow submit \
  --url=https://stepflow.company.com/api/v1 \
  --flow=workflow.yaml \
  --inputs=batch-inputs.jsonl \
  --max-concurrent=50
```

The `submit` command uploads the workflow and inputs, then waits for completion and returns results.

## Batch Execution Patterns

### 1. Data Processing Pipeline

Process large datasets in parallel:

```yaml
# data-pipeline.yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "Data Processing Pipeline"
description: "Process data items in parallel"

schemas:
  type: object
  properties:
    input:
      type: object
      properties:
        item_id:
          type: string
        data:
          type: object
      required: ["item_id", "data"]

steps:
  - id: validate
    component: /data/validate
    input:
      data: { $input: "data" }

  - id: transform
    component: /data/transform
    input:
      data: { $step: validate, path: "validated_data" }

  - id: enrich
    component: /data/enrich
    input:
      data: { $step: transform, path: "transformed_data" }

output:
  item_id: { $input: "item_id" }
  processed_data: { $step: enrich, path: "enriched_data" }
```

```jsonl
{"item_id": "item1", "data": {"value": 100, "category": "A"}}
{"item_id": "item2", "data": {"value": 200, "category": "B"}}
{"item_id": "item3", "data": {"value": 150, "category": "A"}}
```

```bash
stepflow run \
  --flow=data-pipeline.yaml \
  --inputs=items.jsonl \
  --max-concurrent=20 \
  --output=processed-items.jsonl
```

### 2. Bulk AI Analysis

Analyze multiple documents or items with AI:

```yaml
# ai-analysis.yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "AI Document Analysis"

schemas:
  type: object
  properties:
    input:
      type: object
      properties:
        document_id:
          type: string
        content:
          type: string
        analysis_type:
          type: string
          enum: ["sentiment", "summary", "classification"]
      required: ["document_id", "content", "analysis_type"]

steps:
  - id: analyze
    component: /builtin/openai
    input:
      messages:
        - role: system
          content: "You are a document analysis assistant."
        - role: user
          content:
            { $input: "content" }
            transform: "Perform " + $.analysis_type + " analysis on: " + x
      model: "gpt-4"
      temperature: 0.1

output:
  document_id: { $input: "document_id" }
  analysis_type: { $input: "analysis_type" }
  result: { $step: analyze, path: "response" }
```

```bash
# Process 1000 documents with controlled concurrency
stepflow run \
  --flow=ai-analysis.yaml \
  --inputs=documents.jsonl \
  --max-concurrent=10 \
  --output=analysis-results.jsonl
```

### 3. Parallel Testing

Run the same workflow with different test inputs:

```yaml
# test-workflow.yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "API Integration Test"

schemas:
  type: object
  properties:
    input:
      type: object
      properties:
        test_case:
          type: string
        endpoint:
          type: string
        expected_status:
          type: integer
      required: ["test_case", "endpoint", "expected_status"]

steps:
  - id: call_api
    component: /http/request
    input:
      url: { $input: "endpoint" }
      method: "GET"

  - id: validate_response
    component: /test/validate
    input:
      actual_status: { $step: call_api, path: "status_code" }
      expected_status: { $input: "expected_status" }

output:
  test_case: { $input: "test_case" }
  passed: { $step: validate_response, path: "passed" }
  details: { $step: validate_response, path: "details" }
```

```jsonl
{"test_case": "health_check", "endpoint": "https://api.example.com/health", "expected_status": 200}
{"test_case": "not_found", "endpoint": "https://api.example.com/invalid", "expected_status": 404}
{"test_case": "user_list", "endpoint": "https://api.example.com/users", "expected_status": 200}
```

```bash
stepflow run \
  --flow=test-workflow.yaml \
  --inputs=test-cases.jsonl \
  --max-concurrent=5
```

### 4. Batch with User-Defined Components

Use Python SDK components for programmatic batch execution:

```python
from stepflow_server import StepflowStdioServer, StepflowContext
import msgspec

class Input(msgspec.Struct):
    items: list[dict]
    workflow_id: str
    max_concurrency: int = 10

class Output(msgspec.Struct):
    results: list[dict]
    total_processed: int
    success_count: int
    error_count: int

server = StepflowStdioServer()

@server.component
async def batch_processor(input: Input, ctx: StepflowContext) -> Output:
    """Process a batch of items using a sub-workflow"""

    # Submit batch using the runs/submit protocol method
    run_status = await ctx.submit_run(
        flow_id=input.workflow_id,
        inputs=input.items,
        wait=True,
        max_concurrency=input.max_concurrency
    )

    # Aggregate results
    results = run_status.results or []
    success_count = run_status.items.success
    error_count = run_status.items.failed

    return Output(
        results=[r.result for r in results],
        total_processed=len(results),
        success_count=success_count,
        error_count=error_count
    )

if __name__ == "__main__":
    server.run()
```

Use in a workflow:

```yaml
# main-workflow.yaml
schema: https://stepflow.org/schemas/v1/flow.json
steps:
  - id: load_items
    component: /data/load_items
    input:
      source: "database"

  - id: process_batch
    component: /python/batch_processor
    input:
      items: { $step: load_items, path: "items" }
      workflow_id: "sha256:abc123..."
      max_concurrency: 20

  - id: aggregate_results
    component: /data/aggregate
    input:
      results: { $step: process_batch, path: "results" }
```

## Related Documentation

- [CLI: run](../cli/run.md) - Local execution with batch support
- [CLI: submit](../cli/submit.md) - Remote execution with batch support
- [Control Flow](./control-flow.md) - Error handling in workflows
- [Runtime Overrides](./overrides.md) - Batch-specific configuration
- [Configuration](../configuration.md) - Distributed execution setup
