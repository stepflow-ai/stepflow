---
sidebar_position: 5
---

# Control Flow

Control flow in StepFlow determines how workflow execution proceeds - from basic step execution order to conditional execution, skipping, and error handling.

## Execution

Step execution order is determined by data dependencies, not the order in the YAML file. StepFlow automatically analyzes references between steps to determine optimal execution order and enables parallel execution when possible.

```yaml
steps:
  # Steps 1 & 2: No dependencies - run in parallel
  - id: load_config
    component: /builtin/load_file
    input:
      path: "config.json"

  - id: load_data
    component: /builtin/load_file
    input:
      path: "data.json"

  # Step 3: Depends on both above steps
  - id: validate_data
    component: /data/validate
    input:
      data: { $from: { step: load_data } }
      schema: { $from: { step: load_config }, path: "validation_schema" }
```

**Execution flow**: `load_config` and `load_data` start simultaneously, then `validate_data` waits for both to complete.

### Step States

During exuecution, steps transition through a variety of states, as shown below:

**Runtime States:**
- **Pending**: Waiting for dependencies or resources
- **Running**: Currently executing

**Terminal States:**
- **Success**: Completed successfully with output
- **Skipped**: Skipped due to conditions or failed dependencies
- **Failed**: Business logic error (not infrastructure failures)

## Skipping

Steps can be skipped either conditionally (or due to failures) or when their dependencies are skipped.

### Skip Conditions

Steps can be conditionally skipped using the `skip` field:

```yaml
steps:
  - id: expensive_analysis
    component: /ai/analyze
    skip: { $from: { workflow: input }, path: "skip_analysis" }
    input:
      data: { $from: { step: load_data } }
```

- **Reference**: Must reference workflow input or earlier step output
- **Evaluation**: If the referenced value is truthy, step is skipped
- **Default**: `skip: false` (step executes normally)

### Skip Propagation

When a step is skipped, dependent steps are also skipped by default:

```yaml
steps:
  - id: optional_step
    component: /data/process
    skip: { $from: { workflow: input }, path: "skip_optional" }

  # This step skips if optional_step is skipped
  - id: dependent_step
    component: /data/analyze
    input:
      data: { $from: { step: optional_step } }
```

### Handling Skipped Dependencies

Use `$on_skip` to provide fallback behavior when referenced steps are skipped:

```yaml
steps:
  - id: consumer_step
    component: /data/process
    input:
      required_data: { $from: { step: required_step } }
      optional_data:
        $from: { step: optional_step }
        onSkip:
          action: useDefault
          defaultValue: it was skipped
```

**Actions:**
- **`skip_step`** (default): Skip the consuming step
- **`use_default`**: Use the provided `$default` value

## Errors

### Error Handling

Steps can handle failures gracefully using `on_error`:

```yaml
steps:
  - id: risky_operation
    component: /external/api_call
    onError:
      action: useDefault
      defaultValue:
        result: "fallback_value"
        status: "error_handled"
    input:
      endpoint: "https://api.example.com"
```

### Error Actions

- **`terminate`** (default): Stop workflow execution
- **`useDefault`**: Use `defaultValue` and mark step as successful
- **`skip`**: Mark step as skipped (triggers skip propagation)

### Default Output Requirements

When using `action: useDefault`, the `defaultValue` must:
- Conform to the component's output schema
- Include all required fields
- Use compatible data types

## Advanced Patterns

### Conditional Logic

```yaml
steps:
  - id: check_user_level
    component: /user/check_level
    input:
      user_id: { $from: { workflow: input }, path: "user_id" }

  # Different processing based on user level
  - id: premium_analysis
    component: /analytics/premium
    skip: { $from: { step: check_user_level }, path: "is_basic_user" }
    input:
      user_data: { $from: { step: check_user_level } }

  - id: basic_analysis
    component: /analytics/basic
    skip: { $from: { step: check_user_level }, path: "is_premium_user" }
    input:
      user_data: { $from: { step: check_user_level } }
```

### Graceful Degradation

```yaml
steps:
  - id: critical_data
    component: /data/load_critical
    # No error handling - must succeed

  - id: enhancement_data
    component: /data/load_enhancement
    onError:
      action: useDefault
      defaultValue: { enhanced: false }

  - id: process_data
    component: /data/process
    input:
      critical: { $from: { step: critical_data } }
      enhanced:
        $from: { step: enhancement_data }
        onSkip:
          action: useDefault
          defaultValue: { enhanced: false }
```