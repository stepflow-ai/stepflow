---
sidebar_position: 2
---

# Steps

Steps are the fundamental building blocks of Stepflow workflows.
Each step executes a specific component and produces output that can be used by subsequent steps.

The following example is a minimal step -- a name for referencing it's results, the component to execute, and a description of the input to the component.

```yaml title="Minimal Step"
  - id: step_name
    component: /path/to/component
    input:
      a: 1
      b: { $from: { workflow: input }, path: "input_value" }
      c: { $from: { step: previous_step }, path: "result_field" }
```

As you can see, inputs can reference flow inputs, step outputs, or literal values.
See [Expressions](./expressions.md) for more details on how these references can be created.

## Skipping Steps

Steps may optionally define a `skip` condition which determines whether the step should be executed or skipped.
This should be an expression that evaluates to a boolean value.

:::warning
If a step is skipped, any later steps that reference it will be skipped by default.
See [Control Flow](./control-flow.md#skipping) for more details on control flow and how to handle skipped steps.
:::

## Error Handling

By default, an error in a step causes a failure that terminates the entire workflow.
However, steps can specify `on_error` to configure different error handling.

```yaml title="Error Handling examlpe"
steps:
  - id: risky_operation
    component: /external/api_call
    # highlight-start
    on_error:
      action: continue
      default_output:
        result: "fallback_value"
        status: "error_handled"
    # highlight-end
    input:
      endpoint: "https://api.example.com"
```

### Error Action

The `action` may be one of the following:

- **`terminate`** (default): Stop workflow execution
- **`continue`**: Use `default_output` and mark step as successful
- **`skip`**: Mark step as skipped (triggers skip propagation)

In the future, this may include options for retrying failed steps.

:::note
When using `action: continue`, the `default_output` must be set and produce a value that conforms to the component's output schema.
The default output may be an [expression](./expressions.md) that references earlier steps.
:::

See [Control Flow](./control-flow.md#errors) for more details on error handling in steps.

## Next Steps

- For more information on available components and creating your own, see [Components](../components/index.md)
- For more details on how steps are executed, see [Control Flow](./control-flow.md)