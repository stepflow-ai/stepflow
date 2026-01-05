---
sidebar_position: 7
---

# Runtime Overrides

Runtime overrides allow you to modify workflow behavior at execution time without changing the original workflow definition. This powerful feature enables debugging, A/B testing, and dynamic workflow customization.

:::tip When to Use Overrides vs. Variables
**Overrides** are best for **ad-hoc or impromptu changes** during development, testing, or debugging:
- Testing different component implementations
- Skipping expensive steps during development
- Debugging with fixed values
- A/B testing variations

**Variables** are better for **intentional, planned configuration** that varies by environment:
- API endpoints and credentials
- Environment-specific settings (dev/staging/prod)
- Feature flags and resource limits

See [Variables](./variables.md) for environment-specific configuration.
:::

## Overview

Overrides work by:
1. **Specification**: Provide overrides via CLI flags, API parameters or configuration files
2. **Application**: Stepflow applies overrides before workflow execution
3. **Precedence**: Override values take precedence over workflow definitions
4. **Scope**: Overrides can modify step inputs, components, skip conditions, and error handling

## Override Structure

Overrides are specified as a mapping from step IDs to override values:

```yaml
step_id:
  value:
    input:        # Override step input
      param1: new_value
    component: /new/component  # Change component
    skip: true    # Override skip condition
    onError:      # Override error handling
      action: useDefault
      defaultValue: { result: "fallback" }
```

## Common Use Cases

### 1. Environment-Specific Configuration

Different settings for development, staging, and production:

```yaml
# dev-overrides.yaml
ai_analysis:
  value:
    input:
      model: "gpt-3.5-turbo"  # Faster, cheaper model for dev
      temperature: 0.7
      max_tokens: 100

data_validation:
  value:
    skip: true  # Skip expensive validation in dev
```

```yaml
# prod-overrides.yaml
ai_analysis:
  value:
    input:
      model: "gpt-4"  # Production-quality model
      temperature: 0.1
      max_tokens: 500

data_validation:
  value:
    skip: false  # Always validate in production
```

```bash
# Development
stepflow run --flow=workflow.yaml --input=input.json --overrides=dev-overrides.yaml

# Production
stepflow run --flow=workflow.yaml --input=input.json --overrides=prod-overrides.yaml
```

### 2. A/B Testing

Test different parameter values or components:

```yaml
# variant-a-overrides.yaml
recommendation_engine:
  value:
    input:
      algorithm: "collaborative_filtering"
      min_confidence: 0.8

# variant-b-overrides.yaml
recommendation_engine:
  value:
    input:
      algorithm: "content_based"
      min_confidence: 0.7
```

### 3. Component Substitution

Swap components for testing or different implementations:

```yaml
# Use mock component for testing
external_api_call:
  value:
    component: /mock/api_response
    input:
      mock_data: { status: "success", data: [...] }

# Use alternative implementation
data_processor:
  value:
    component: /python/optimized_processor  # Faster version
```

### 4. Dynamic Skip Conditions

Control which steps execute:

```yaml
# Skip expensive operations for quick testing
expensive_analysis:
  value:
    skip: true

ml_training:
  value:
    skip: true

# Or enable optional features
optional_enrichment:
  value:
    skip: false
```

### 5. Debugging and Isolation

Override step inputs with fixed values to isolate and debug specific parts of a workflow. When you override all references to a step, that step won't execute, allowing you to test downstream logic without running expensive or problematic steps:

```yaml
# debug-overrides.yaml
# Skip expensive data loading by providing mock data directly
process_data:
  value:
    input:
      # Override with known good data from previous run
      data: { $step: load_data }  # Original reference
      
# Replace the reference with a literal value
process_data:
  value:
    input:
      data:  # Now using literal data instead of step reference
        items: [
          {"id": 1, "value": 100},
          {"id": 2, "value": 200}
        ]
        # load_data step won't execute since nothing references it

# Debug AI step with fixed input
ai_analysis:
  value:
    input:
      prompt: "Analyze this specific test case"
      temperature: 0.0  # Deterministic for debugging
      
# Skip problematic step entirely
problematic_step:
  value:
    skip: true
```

**Debugging workflow:**
```bash
# Run with debug overrides to isolate issue
stepflow run --flow=workflow.yaml --input=input.json \
  --overrides=debug-overrides.yaml
```

This technique is particularly useful for:
- **Reproducing issues**: Use exact data from a failed run
- **Testing downstream logic**: Skip early steps with known outputs
- **Isolating failures**: Bypass problematic steps to test the rest of the workflow
- **Faster iteration**: Avoid running expensive operations during development

### 6. Error Handling Customization

Adjust error handling behavior:

```yaml
# More lenient error handling for development
external_service:
  value:
    onError:
      action: useDefault
      defaultValue: { status: "unavailable" }

# Strict error handling for production
critical_validation:
  value:
    onError:
      action: terminate  # Fail fast in production
```

## Override Methods

### CLI Inline Overrides

**JSON format:**
```bash
stepflow run --flow=workflow.yaml --input=input.json \
  --overrides-json '{"ai_step": {"value": {"input": {"temperature": 0.9}}}}'
```

**YAML format:**
```bash
stepflow run --flow=workflow.yaml --input=input.json \
  --overrides-yaml 'ai_step: {value: {input: {temperature: 0.9}}}'
```

### File-Based Overrides

**JSON file:**
```json
{
  "ai_step": {
    "value": {
      "input": {
        "temperature": 0.9,
        "max_tokens": 200
      }
    }
  },
  "validation_step": {
    "value": {
      "skip": true
    }
  }
}
```

**YAML file:**
```yaml
ai_step:
  value:
    input:
      temperature: 0.9
      max_tokens: 200

validation_step:
  value:
    skip: true
```

```bash
stepflow run --flow=workflow.yaml --input=input.json --overrides=overrides.yaml
```

### Programmatic Overrides (Python SDK)

Use overrides when executing workflows programmatically via the Python SDK:

```python
from stepflow_server import StepflowContext, Flow
import asyncio

async def execute_with_overrides():
    async with StepflowContext.from_config("stepflow-config.yml") as ctx:
        # Load workflow
        flow = Flow.from_file("workflow.yaml")
        
        # Define overrides programmatically
        overrides = {
            "ai_step": {
                "value": {
                    "input": {
                        "temperature": 0.9,
                        "max_tokens": 200
                    }
                }
            },
            "validation_step": {
                "value": {
                    "skip": True
                }
            }
        }
        
        # Execute with overrides
        result = await ctx.evaluate_flow(
            flow=flow,
            input={"query": "What is AI?"},
            overrides=overrides
        )
        
        print(result)

asyncio.run(execute_with_overrides())
```

**Batch execution with overrides:**
```python
async def batch_with_overrides():
    async with StepflowContext.from_config("config.yml") as ctx:
        flow = Flow.from_file("workflow.yaml")
        
        # Apply same overrides to all batch items
        overrides = {
            "ai_step": {
                "value": {
                    "input": {
                        "model": "gpt-3.5-turbo",
                        "temperature": 0.1
                    }
                }
            }
        }
        
        inputs = [
            {"query": "Question 1"},
            {"query": "Question 2"},
            {"query": "Question 3"}
        ]
        
        # Execute batch with overrides
        results = await ctx.evaluate_batch(
            flow=flow,
            inputs=inputs,
            max_concurrency=10,
            overrides=overrides
        )
        
        return results

asyncio.run(batch_with_overrides())
```

**Dynamic overrides based on conditions:**
```python
async def dynamic_overrides(use_production_model: bool):
    async with StepflowContext.from_config("config.yml") as ctx:
        flow = Flow.from_file("workflow.yaml")
        
        # Choose overrides based on runtime conditions
        if use_production_model:
            overrides = {
                "ai_step": {
                    "value": {
                        "input": {
                            "model": "gpt-4",
                            "temperature": 0.1
                        }
                    }
                }
            }
        else:
            overrides = {
                "ai_step": {
                    "value": {
                        "input": {
                            "model": "gpt-3.5-turbo",
                            "temperature": 0.7
                        }
                    }
                }
            }
        
        result = await ctx.evaluate_flow(
            flow=flow,
            input={"query": "Analyze this data"},
            overrides=overrides
        )
        
        return result
```

## Advanced Patterns

### Conditional Overrides Based on Environment

Use environment variables to select override files:

```bash
# Set environment
export ENVIRONMENT=production

# Use environment-specific overrides
stepflow run --flow=workflow.yaml --input=input.json \
  --overrides=${ENVIRONMENT}-overrides.yaml
```

### Layered Overrides

Combine multiple override sources (later overrides take precedence):

```bash
# Base overrides + environment-specific overrides
stepflow run --flow=workflow.yaml --input=input.json \
  --overrides=base-overrides.yaml \
  --overrides-json '{"specific_step": {"value": {"input": {"param": "value"}}}}'
```

### Override Templates

Create reusable override templates:

```yaml
# fast-execution-overrides.yaml
# Template for quick testing with minimal resource usage
ai_analysis:
  value:
    input:
      model: "gpt-3.5-turbo"
      max_tokens: 50

data_processing:
  value:
    input:
      batch_size: 10
      timeout: 30

expensive_validation:
  value:
    skip: true
```

### Batch-Specific Overrides

Apply consistent settings across batch executions:

```yaml
# batch-overrides.yaml
# Optimized for high-throughput batch processing
process_item:
  value:
    input:
      batch_size: 100
      concurrency: 10
      cache_enabled: true

ai_analysis:
  value:
    input:
      model: "gpt-3.5-turbo"  # Faster model for batch
      temperature: 0.1  # More consistent results
```

```bash
stepflow run --flow=workflow.yaml --inputs=batch-inputs.jsonl \
  --overrides=batch-overrides.yaml --max-concurrent=20
```

## Override Precedence

When multiple override sources are provided, they are applied in order:

1. **Workflow definition** (base values)
2. **Override file** (if provided)
3. **Inline overrides** (--overrides-json or --overrides-yaml)

Later overrides take precedence over earlier ones.

## Related Documentation

- [Variables](./variables.md) - Environment-specific workflow parameters
- [CLI: run](../cli/run.md) - Local workflow execution with overrides
- [CLI: submit](../cli/submit.md) - Remote workflow submission with overrides
- [Control Flow](./control-flow.md) - Skip conditions and error handling
- [Configuration](../configuration.md) - Environment-specific configuration