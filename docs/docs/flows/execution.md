---
sidebar_position: 6
---

# Execution

Understanding how StepFlow executes workflows is crucial for writing efficient, reliable workflows. This guide covers the execution model, states, and fundamental execution patterns.

## Execution Model

### Dependency-Based Execution

StepFlow uses a dependency-driven execution model rather than sequential execution:

```yaml
steps:
  # Step 1: No dependencies - starts immediately
  - id: load_config
    component: /builtin/load_file
    input:
      path: "config.json"

  # Step 2: No dependencies - runs in parallel with load_config
  - id: load_data
    component: /builtin/load_file
    input:
      path: "data.json"

  # Step 3: Depends on load_data - waits for it to complete
  - id: validate_data
    component: /data/validate
    input:
      data: { $from: { step: load_data } }
      schema: { $from: { step: load_config }, path: "validation_schema" }

  # Step 4: Depends on validate_data - runs after validation
  - id: process_data
    component: /data/process
    input:
      validated_data: { $from: { step: validate_data } }
```

**Execution Flow:**
1. `load_config` and `load_data` start simultaneously
2. `validate_data` waits for both to complete
3. `process_data` waits for `validate_data` to complete

### Parallel Execution

Steps without dependencies run in parallel automatically:

```yaml
steps:
  - id: load_user_data
    component: /user/load
    input:
      user_id: { $from: { workflow: input }, path: "user_id" }

  # These three steps run in parallel after load_user_data completes
  - id: analyze_behavior
    component: /analytics/behavior
    input:
      user_data: { $from: { step: load_user_data } }

  - id: check_permissions
    component: /auth/permissions
    input:
      user_data: { $from: { step: load_user_data } }

  - id: load_preferences
    component: /user/preferences
    input:
      user_id: { $from: { step: load_user_data }, path: "id" }
```

## Execution States

### Step States

Each step progresses through these states:

#### Runtime States
- **Pending**: Waiting for dependencies or resources
- **Running**: Currently executing

#### Terminal States
- **Success**: Completed successfully with output
- **Skipped**: Skipped due to conditions or dependencies
- **Failed**: Completed with a business logic error

### Workflow States

- **Running**: One or more steps are executing
- **Completed**: All steps have reached terminal states
- **Failed**: Workflow terminated due to unrecoverable error

## Resource Management

### Concurrency Control

StepFlow manages concurrent execution automatically:

```yaml
# No explicit parallelism needed - StepFlow handles it
steps:
  - id: fetch_data_source_1
    component: /http/get
    input:
      url: "https://api1.example.com/data"

  - id: fetch_data_source_2
    component: /http/get
    input:
      url: "https://api2.example.com/data"

  - id: fetch_data_source_3
    component: /http/get
    input:
      url: "https://api3.example.com/data"

  # Waits for all three fetches to complete
  - id: combine_data
    component: /data/merge
    input:
      source1: { $from: { step: fetch_data_source_1 } }
      source2: { $from: { step: fetch_data_source_2 } }
      source3: { $from: { step: fetch_data_source_3 } }
```

### Memory Management

#### Blob Storage

Use blobs for large or reusable data:

```yaml
steps:
  # Store large dataset in blob
  - id: store_large_dataset
    component: /builtin/put_blob
    input:
      data: { $from: { step: load_massive_dataset } }

  # Multiple steps can reference the same blob efficiently
  - id: analyze_subset_1
    component: /analytics/process
    input:
      data_blob: { $from: { step: store_large_dataset }, path: "blob_id" }
      filter: "category=A"

  - id: analyze_subset_2
    component: /analytics/process
    input:
      data_blob: { $from: { step: store_large_dataset }, path: "blob_id" }
      filter: "category=B"
```

## Advanced Execution Patterns

### Conditional Execution

Implement conditional logic using skip conditions:

```yaml
steps:
  - id: check_user_level
    component: /user/check_level
    input:
      user_id: { $from: { workflow: input }, path: "user_id" }

  # Only run for premium users
  - id: premium_analysis
    component: /analytics/premium
    skipIf:
      $from: { step: check_user_level }
      path: "is_basic_user"
    input:
      user_data: { $from: { step: check_user_level } }

  # Only run for basic users
  - id: basic_analysis
    component: /analytics/basic
    skipIf:
      $from: { step: check_user_level }
      path: "is_premium_user"
    input:
      user_data: { $from: { step: check_user_level } }
```

### Dynamic Workflows

Use nested workflows for dynamic execution:

```yaml
steps:
  - id: determine_workflow_type
    component: /workflow/classifier
    input:
      request: { $from: { workflow: input } }

  - id: execute_dynamic_workflow
    component: /builtins/eval
    input:
      workflow: { $from: { step: determine_workflow_type }, path: "workflow_definition" }
      input: { $from: { workflow: input } }
```

## Error Handling and Recovery

### Graceful Degradation

Use error handling to maintain workflow resilience:

```yaml
steps:
  # Critical step - must succeed
  - id: authenticate_user
    component: /auth/verify
    input:
      token: { $from: { workflow: input }, path: "auth_token" }

  # Optional enhancement - can fail gracefully
  - id: enrich_profile
    component: /user/enrich
    on_error:
      action: continue
      default_output:
        enriched: false
        additional_data: {}
    input:
      user_id: { $from: { step: authenticate_user }, path: "user_id" }

  # Main processing - uses enriched data if available
  - id: generate_response
    component: /response/create
    input:
      user_data: { $from: { step: authenticate_user } }
      enriched_data:
        $from: { step: enrich_profile }
        $on_skip: "use_default"
        $default: { enriched: false }
```

### Error Propagation

Control how errors propagate through the workflow:

```yaml
steps:
  # Optional data source - skip if it fails
  - id: load_optional_data
    component: /data/load
    on_error:
      action: skip
    input:
      source: "optional_data_source"

  # Processing step - handles missing optional data
  - id: process_data
    component: /data/process
    input:
      required_data: { $from: { step: load_required_data } }
      optional_data:
        $from: { step: load_optional_data }
        $on_skip: "use_default"
        $default: {}
```

## Next Steps

For more advanced topics related to workflow execution, see:

- **[Performance Optimization](./performance.md)** - Strategies for maximizing workflow performance and parallelism
- **[Best Practices](./best-practices.md)** - Design principles and patterns for reliable workflows
- **[Testing](./testing.md)** - Comprehensive testing strategies for workflow validation

Understanding these execution fundamentals provides the foundation for building efficient, reliable StepFlow workflows.