---
sidebar_position: 3
---

# Expressions

Stepflow's expression system enables dynamic data references within flows.
Expressions are used to specify the input and skip condition of steps as well as the output of workflows and in various other places.
They allow creating a value that is entirely or partially derived from other data in the workflow.

## Expression Types

| Expression | Description |
|---|---|
| [Bare literals](#bare-literals) | Any JSON value used directly |
| [`$input`](#references) | Reference to workflow input |
| [`$step`](#references) | Reference to a step's output |
| [`$variable`](#references) | Reference to a workflow variable |
| [`$literal`](#escaped-literals) | Escaped literal (prevents expression evaluation) |
| [`$if`](#conditional-if) | Conditional expression |
| [`$coalesce`](#coalesce) | Returns first non-null value |

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

Value reference allow you to dynamically reference data from earlier in the flow using special `$` prefixed keys.

Some expressions may also have an optional `path` field that is applied to the referenced value.
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
If you want to use an object that contains special `$` prefixed keys without being a reference, see [Escaped Literals](#escaped-literals).
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

## Conditional ($if) {#conditional-if}

The `$if` expression evaluates a condition and returns one of two values.

```yaml
# Basic conditional
result:
  $if: { $variable: debug_mode }
  then: { log_level: debug }
  else: { log_level: info }

# Conditional with step reference
output:
  $if: { $step: check_step, path: "success" }
  then: { $step: success_step }
  else: { $step: fallback_step }
```

The `else` field is optional. If omitted and the condition is falsy, the expression evaluates to `null`.

| Field | Required | Description |
|---|---|---|
| `$if` | Yes | Expression that evaluates to a boolean |
| `then` | Yes | Value to use if condition is truthy |
| `else` | No | Value to use if condition is falsy (defaults to `null`) |

## Coalesce ($coalesce) {#coalesce}

The `$coalesce` expression returns the first non-null value from a list of expressions.
This is useful for providing fallbacks when a value may not be available.

```yaml
# Return first available value
api_url:
  $coalesce:
    - { $variable: custom_api_url }   # use custom URL if set
    - { $variable: default_api_url }  # fall back to default
    - "https://api.example.com"       # hardcoded fallback

# Coalesce with step references
result:
  $coalesce:
    - { $step: primary_step }
    - { $step: fallback_step }
```

If all values in the list are `null`, the expression evaluates to `null`.

## Next Steps

- See [./control-flow.md](./control-flow.md) to learn about skip conditions and error handling
