# Control Flow

This document describes the implementation of control flow features in Step Flow, including conditional execution, error handling, and nested workflow evaluation.

## Overview

Step Flow supports advanced control flow patterns through:
- **Conditional Execution**: Steps can be skipped based on runtime conditions
- **Error Handling**: Steps can handle failures gracefully with fallback values
- **Nested Workflows**: Sub-workflows can be executed within steps

## Step States

Steps in Step Flow have the following states:

### Execution States (tracked by execution engine)
- **Pending**: Not yet executed, waiting for inputs or resources
- **Running**: Currently executing

### Terminal States (returned in FlowResult)
- **Success**: Completed successfully and produced output
- **Skipped**: Skipped due to conditions or dependencies, with optional reason
- **Failed**: Completed with a business logic error (FlowError)

Note: Internal errors (infrastructure failures, plugin communication issues) are handled by the workflow runtime and are not represented as step failures.

## Conditional Execution

Steps can be conditionally skipped using the `skip` field:

```yaml
steps:
  - id: expensive_analysis
    component: ai/analyze
    skip: { $ref: { workflow: input }, path: is_premium_user }
    args:
      data: { $ref: { step: previous_step } }
```

### Skip Conditions
- **Reference**: Must be a workflow input or output from an earlier step
- **Evaluation**: Evaluated at runtime when step dependencies are ready
- **Logic**: If the referenced value is truthy, the step is skipped
- **Default**: `skip: false` (step executes normally)

### Skip Propagation

By default, if any inputs to a step are unavailable because the producing step was skipped, the consuming step is also skipped.
This propagation can be interrupted by configuring how an input should be computed when the producer was skipped:

```yaml
steps:
  - id: consumer_step
    component: data/process
    args:
      required_data: { $ref: { step: step1 } }
      optional_enhancement:
        $ref: { step: step2 }
        $on_skip: "use_default"
        $default: "no_enhancement"
```

The `$on_skip` field configures what to do when the referenced step was skipped.
It defaults to `skip_step` which causes the entire consuming step to be skipped.
It can be set to `use_default` which causes a default value to be used instead.

The `$default` field is required if `$on_skip` is set to `use_default` and the type of the argument does not have a natural default value.

## Error Handling

Steps can handle their own failures using the `on_error` field:

```yaml
steps:
  - id: risky_operation
    component: external/api_call
    on_error:
      action: continue
      default_output:
        result: "fallback_value"
        status: "error_handled"
    args:
      endpoint: "https://api.example.com"
```

### Error Actions
- **terminate**: Terminate the workflow (default behavior)
- **continue**: Use `default_output` as the step result (Success state)
- **skip**: Mark the step as Skipped

### Default Output Requirements
- Must conform to the component's output schema
- Required fields must be present
- Optional fields may be omitted
- Type compatibility is enforced during validation

## Nested Workflows (Eval)

Sub-workflows can be executed using the built-in `eval` component:

```yaml
steps:
  - id: sub_process
    component: builtin://eval
    args:
      workflow:
        inputs:
          data: { type: string }
        steps:
          - id: process
            component: data/transform
            args:
              input: { $ref: { workflow: input }, path: data }
        outputs:
          result: { $ref: { step: process } }
      input:
        data: { $ref: { step: previous_step }, path: processed_data }
```

### Sub-workflow Execution
- **Isolation**: Each sub-workflow gets a unique execution ID
- **Context**: Completely isolated from parent workflow state
- **Resources**: Count against parent workflow resource limits
- **Errors**: Sub-workflow failures propagate as eval step failures

### Use Cases
- Factoring complex workflows into reusable components
- Dynamic workflow execution based on runtime data
- Implementing complex looping and branching logic

### Resource Management
- Sub-workflows inherit resource limits from parent
- Failed sub-workflows don't consume additional resources
- Skipped steps don't consume execution resources

## Future Enhancements

- **Retry Logic**: `on_error: {action: retry, max_attempts: 3}`
- **Time Limits**: Step- or edge-level timeouts with skip behavior
- **Loop Primitives**: Tail-eval for iterative workflows
- **Fuel/Resource System**: Execution step limits for loop termination