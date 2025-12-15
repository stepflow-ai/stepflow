---
sidebar_position: 3
---

# Expressions

Stepflow's expression system enables dynamic data references within flows.
Expressions are used to specify the input and skip condition of steps as well as the output of workflows and in various other places.
They allow creating a value that is entirely or partially derived from other data in the workflow.

## Bare Literals

The simplest expression is a bare literal.
Every valid JSON value is a valid value in Stepflow.

:::note
Depending on the flow format, this may be written in YAML syntax.
:::

```yaml
value:
  string_value: "Hello, World!"
  number_value: 42
  boolean_value: true
  null_value: null
  object_value:
    key1: "value1"
    key2: 2
    key3: false
  array_value:
    - "item1"
    - 2
    - true
```

## References {#references}

Value references allow you to dynamically reference data from earlier in the flow using special `$` prefixed keys.

The object may also have an optional `path` field that is applied to the referenced value.
This may be the name of a field (assuming the value is an object) or a JSON path expression starting with `$`.

```yaml
reference_examples:
  # Referencing workflow input
  - { $input: user }              # just user field from workflow input
  - { $input: "$.user.name" }     # just user.name field using JSONPath

  # Referencing step output
  - { $step: step1 }                          # entire output of step1
  - { $step: step1, path: "result" }          # result field of step1 output
  - { $step: step1, path: "$.data.items[0]" } # first item in data.items array

  # Referencing variables
  - { $variable: api_endpoint }   # entire variable value
  - { $variable: api_key }        # variable (often used for secrets)
```

See [Variables](./variables.md) for comprehensive documentation on using variables in workflows.

:::tip[Escaping Literals]
If you want to use an object that contains special `$` prefixed keys without evaluating the reference, see [Escaped Literals](#escaped-literals).
:::

:::note[Skip Handling]
When referencing a step that has been skipped, the default behavior is to skip the step.
The reference may specify an `on_skip` action to change this behavior.
See [Handling Skipped Dependencies](./control-flow.md#handling-skipped-dependencies) for more details.
:::

## Escaped Literals

However, bare literals like this have an issue if you are trying to create a JSON value that looks like an expression.
For example, if you are trying to create a value that represents a flow.
In these cases, the value `{ "$literal": ...value...}` will evaluate to the `...value...` without any expressions being processed.

```yaml
value_with_expressions:
  a: { $step: some_step }
  b:
    $literal:
      c: { $step: another_step }
```

If `some_step` produced the value `{ "x": 10", "y": "hello" }`, then the result of the above expression is:

```json
{
  "a": { "x": 10, "y": "hello" },
  "b": {
    "c": { "$step": "another_step" }
  }
}
```

Note the first expression was evaluated while the second was preserved as a literal value.

## Next Steps

- See [./control-flow.md](./control-flow.md) to learn about skip conditions and error handling