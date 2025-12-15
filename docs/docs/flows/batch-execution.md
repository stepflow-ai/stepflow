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

Execute batches locally using the `run-batch` command:

```bash
stepflow run-batch --flow=workflow.yaml --inputs=batch-inputs.jsonl
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
stepflow run-batch \
  --flow=data-pipeline.yaml \
  --inputs=items.jsonl \
  --output=results.jsonl
```

### Concurrency Control

Limit concurrent executions to manage resource usage:

```bash
# Limit to 10 concurrent executions
stepflow run-batch \
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
stepflow submit-batch \
  --url=https://stepflow.company.com/api/v1 \
  --flow=workflow.yaml \
  --inputs=batch-inputs.jsonl \
  --max-concurrent=50
```

### Batch Tracking

Monitor batch progress and retrieve results:

```bash
# Submit batch and get batch ID
BATCH_ID=$(stepflow submit-batch \
  --url=$STEPFLOW_URL \
  --flow=workflow.yaml \
  --inputs=inputs.jsonl \
  --output=/dev/null | jq -r '.batch_id')

# Check batch status
stepflow get-batch --url=$STEPFLOW_URL --batch-id=$BATCH_ID

# Wait for completion and get results
stepflow get-batch \
  --url=$STEPFLOW_URL \
  --batch-id=$BATCH_ID \
  --wait \
  --output=results.jsonl
```

## Batch Execution Patterns

### 1. Data Processing Pipeline

Process large datasets in parallel:

```yaml
# data-pipeline.yaml
schema: https://stepflow.org/schemas/v1/flow.json
name: "Data Processing Pipeline"
description: "Process data items in parallel"

input_schema:
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
stepflow run-batch \
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

input_schema:
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
            { $input }
            path: "content"
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
stepflow run-batch \
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

input_schema:
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
stepflow run-batch \
  --flow=test-workflow.yaml \
  --inputs=test-cases.jsonl \
  --max-concurrent=5
```

### 4. Batch with User-Defined Components

Use Python SDK's `evaluate_batch` for programmatic batch execution:

```python
from stepflow_py import StepflowContext, Flow
import asyncio

async def process_batch():
    async with StepflowContext.from_config("stepflow-config.yml") as ctx:
        # Load workflow
        flow = Flow.from_file("workflow.yaml")
        
        # Prepare batch inputs
        inputs = [
            {"item_id": f"item{i}", "value": i * 10}
            for i in range(100)
        ]
        
        # Execute batch with concurrency control
        results = await ctx.evaluate_batch(
            flow=flow,
            inputs=inputs,
            max_concurrency=20
        )
        
        # Process results
        for i, result in enumerate(results):
            print(f"Item {i}: {result}")

asyncio.run(process_batch())
```

## Batch Execution Within Workflows

Execute batches from within a workflow using custom components:

```python
# batch_processor.py
from stepflow_py import StepflowStdioServer, StepflowContext
import msgspec

class Input(msgspec.Struct):
    items: list[dict]
    workflow_path: str
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
    from stepflow_py import Flow
    
    # Load the sub-workflow
    flow = Flow.from_file(input.workflow_path)
    
    # Execute batch
    results = await ctx.evaluate_batch(
        flow=flow,
        inputs=input.items,
        max_concurrency=input.max_concurrency
    )
    
    # Aggregate results
    success_count = sum(1 for r in results if r.get("success", False))
    error_count = len(results) - success_count
    
    return Output(
        results=results,
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
      workflow_path: "item-processor.yaml"
      max_concurrency: 20

  - id: aggregate_results
    component: /data/aggregate
    input:
      results: { $step: process_batch, path: "results" }
```

## Related Documentation

- [CLI: run-batch](../cli/run-batch.md) - Local batch execution
- [CLI: submit-batch](../cli/submit-batch.md) - Remote batch execution
- [Control Flow](./control-flow.md) - Error handling in workflows
- [Runtime Overrides](./overrides.md) - Batch-specific configuration
- [Configuration](../configuration.md) - Distributed execution setup