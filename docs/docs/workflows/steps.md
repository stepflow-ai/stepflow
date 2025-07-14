---
sidebar_position: 3
---

# Steps

Steps are the fundamental building blocks of StepFlow workflows. Each step executes a specific component and produces output that can be used by subsequent steps.

## Basic Step Structure

A step has the following structure:

```yaml
steps:
  - id: step_name
    component: component_url
    input:
      # Input data for the component
    # Optional fields:
    skip: condition           # Skip this step if condition is truthy
    on_error:                # How to handle errors
      action: continue|skip|terminate
      default_output: {}     # Default output if action is continue
```

## Step Anatomy

### Required Fields

- **`id`**: Unique identifier for the step within the workflow
- **`component`**: URL specifying which component to execute
- **`input`**: Data to pass to the component

### Optional Fields

- **`skip`**: Condition to skip step execution
- **`on_error`**: Error handling configuration
- **`description`**: Human-readable description

## Step Execution

### Execution Order

Steps execute based on their dependencies, not their order in the YAML file. StepFlow automatically determines the execution order by analyzing data dependencies.

```yaml
steps:
  # This step runs first (no dependencies)
  - id: load_data
    component: load_file
    input:
      path: "data.json"

  # This step waits for load_data to complete
  - id: process_data
    component: /python/udf
    input:
      data: { $from: { step: load_data } }

  # This step also waits for load_data (runs in parallel with process_data)
  - id: validate_data
    component: validate
    input:
      data: { $from: { step: load_data } }

  # This step waits for both process_data and validate_data
  - id: combine_results
    component: merge
    input:
      processed: { $from: { step: process_data } }
      validated: { $from: { step: validate_data } }
```

### Parallel Execution

Steps without dependencies run in parallel automatically. This provides optimal performance without requiring explicit parallelization.

## Data References

Steps reference data using the `$from` syntax:

### Workflow Input

```yaml
input:
  user_id: { $from: { workflow: input }, path: "user_id" }
```

### Step Output

```yaml
input:
  processed_data: { $from: { step: previous_step } }
  # Or reference a specific field
  user_name: { $from: { step: user_lookup }, path: "name" }
```

### Literal Values

```yaml
input:
  config: { $literal: { timeout: 30, retries: 3 } }
  message: { $literal: "Hello World" }
```

### Nested Path References

Use dot notation or array syntax for nested data:

```yaml
input:
  # Dot notation
  user_email: { $from: { step: user_data }, path: "profile.email" }
  # Array index
  first_item: { $from: { step: list_data }, path: "items[0]" }
  # Complex nested path
  deep_value: { $from: { step: complex_data }, path: "level1.level2[0].value" }
```

## Skip Handling

### Basic Skip Condition

```yaml
steps:
  - id: expensive_analysis
    component: ai/analyze
    skip: { $from: { workflow: input }, path: "skip_analysis" }
    input:
      data: { $from: { step: load_data } }
```

### Skip Propagation

When a step is skipped, consuming steps are also skipped by default:

```yaml
steps:
  - id: optional_step
    component: data/process
    skip: { $from: { workflow: input }, path: "skip_optional" }

  # This step is skipped if optional_step is skipped
  - id: dependent_step
    component: data/analyze
    input:
      data: { $from: { step: optional_step } }
```

### Skip Interruption

Use `$on_skip` to provide default values when dependencies are skipped:

```yaml
steps:
  - id: consumer_step
    component: data/process
    input:
      required_data: { $from: { step: required_step } }
      optional_data:
        $from: { step: optional_step }
        $on_skip: "use_default"
        $default: { status: "not_processed" }
```

### Skip Actions

- **`skip_step`** (default): Skip the consuming step
- **`use_default`**: Use the provided default value

## Error Handling

### Error Actions

```yaml
steps:
  - id: risky_operation
    component: external/api_call
    on_error:
      action: continue        # continue|skip|terminate
      default_output:
        result: "fallback_value"
        status: "error_handled"
    input:
      endpoint: "https://api.example.com"
```

### Error Action Types

- **`terminate`** (default): Stop workflow execution
- **`continue`**: Use `default_output` and mark step as successful
- **`skip`**: Mark step as skipped (triggers skip propagation)

### Default Output Requirements

When using `action: continue`, the `default_output` must:
- Conform to the component's output schema
- Include all required fields
- Match expected data types

## Component Types

### Built-in Components

```yaml
# Blob storage
- id: store_data
  component: put_blob
  input:
    data: { $from: { step: prepare_data } }

# Sub-workflow execution
- id: sub_process
  component: eval
  input:
    workflow: { $from: { step: load_workflow } }
    input: { $from: { step: prepare_input } }
```

### External Components

```yaml
# Python UDF
- id: custom_logic
  component: /python/udf
  input:
    blob_id: { $from: { step: create_udf } }
    input: { $from: { workflow: input } }

# Custom component server
- id: specialized_task
  component: my_plugin://advanced_processor
  input:
    configuration: { $from: { step: load_config } }
    data: { $from: { step: prepare_data } }
```

### OpenAI Integration

```yaml
# OpenAI API call
- id: ai_analysis
  component: openai
  input:
    messages:
      - role: system
        content: "You are a helpful assistant."
      - role: user
        content: { $from: { step: user_input }, path: "question" }
    model: "gpt-4"
    max_tokens: 150
```

## Advanced Patterns

### Dynamic Components

Create components at runtime:

```yaml
steps:
  - id: create_custom_component
    component: /factory/create_processor
    input:
      type: "data_transformer"
      config: { $from: { workflow: input }, path: "processor_config" }

  - id: use_custom_component
    component: { $from: { step: create_custom_component }, path: "component_url" }
    input:
      data: { $from: { step: load_data } }
```

### Conditional Processing

```yaml
steps:
  - id: process_if_large
    component: data/heavy_processor
    skip:
      $from: { step: check_size }
      path: "is_small"
    input:
      data: { $from: { step: load_data } }

  - id: process_if_small
    component: data/light_processor
    skip:
      $from: { step: check_size }
      path: "is_large"
    input:
      data: { $from: { step: load_data } }
```

### Multi-step Dependencies

```yaml
steps:
  - id: final_step
    component: data/combine
    input:
      # Wait for multiple steps
      processed: { $from: { step: process_data } }
      validated: { $from: { step: validate_data } }
      enriched: { $from: { step: enrich_data } }
      # With fallback for optional data
      optional_metadata:
        $from: { step: optional_metadata }
        $on_skip: "use_default"
        $default: {}
```

## Best Practices

### Step Naming

- Use descriptive, action-oriented names
- Follow consistent naming conventions
- Avoid generic names like `step1`, `process`

```yaml
# Good
- id: load_user_data
- id: validate_email_format
- id: send_welcome_email

# Avoid
- id: step1
- id: process
- id: do_stuff
```

### Input Organization

- Group related inputs logically
- Use meaningful parameter names
- Provide default values where appropriate

```yaml
# Good organization
input:
  # Data inputs
  user_data: { $from: { step: load_user } }
  settings: { $from: { step: load_settings } }

  # Configuration
  timeout: { $literal: 30 }
  retries: { $literal: 3 }

  # Optional parameters with defaults
  debug_mode:
    $from: { workflow: input }
    path: "debug"
    $on_skip: "use_default"
    $default: false
```

### Error Handling Strategy

- Use `terminate` for critical failures
- Use `continue` for recoverable errors with meaningful defaults
- Use `skip` for optional operations

```yaml
# Critical operation - must succeed
- id: authenticate_user
  component: auth/verify
  # on_error defaults to terminate

# Optional enhancement - can fail gracefully
- id: enrich_profile
  component: data/enrich
  on_error:
    action: continue
    default_output:
      enriched: false
      metadata: {}

# Completely optional - skip if fails
- id: log_analytics
  component: analytics/track
  on_error:
    action: skip
```