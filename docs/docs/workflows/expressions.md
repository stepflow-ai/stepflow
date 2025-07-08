---
sidebar_position: 5
---

# Expressions

StepFlow's expression system enables dynamic data references and transformations within workflows. This comprehensive reference covers all expression types, syntax patterns, and advanced features.

## Expression Overview

Expressions in StepFlow allow you to:
- Reference workflow inputs and step outputs
- Extract specific fields from complex data structures
- Handle missing data gracefully with defaults
- Embed literal values that bypass expression processing

## Core Expression Types

### 1. Data References (`$from`)

Reference data from workflow inputs or step outputs:

```yaml
# Basic reference syntax
{ $from: { workflow: input }, path: "field_name" }
{ $from: { step: step_id }, path: "result.nested_field" }

# Reference entire objects
{ $from: { workflow: input } }
{ $from: { step: previous_step } }
```

### 2. Literal Values (`$literal`)

Embed static values that bypass expression processing:

```yaml
# Simple literals
{ $literal: "static string" }
{ $literal: 42 }
{ $literal: true }

# Complex literals
{ $literal: { key: "value", nested: { data: [1, 2, 3] } } }

# Literals containing special characters
{ $literal: { "$from": "this is not processed as an expression" } }
```

### 3. Skip Handling (`$on_skip`)

Control behavior when referenced data is unavailable:

```yaml
# Use default value when step is skipped
{
  $from: { step: optional_step },
  path: "result",
  $on_skip: "use_default",
  $default: "fallback_value"
}

# Skip consuming step when dependency is skipped
{
  $from: { step: required_step },
  $on_skip: "skip_step"  # This is the default
}
```

## Reference Sources

### Workflow References

```yaml
# Reference workflow input
{ $from: { workflow: input } }

# Reference specific input fields
{ $from: { workflow: input }, path: "user_data" }
{ $from: { workflow: input }, path: "config.database.host" }
```

### Step References

```yaml
# Reference entire step output
{ $from: { step: data_processor } }

# Reference specific output fields
{ $from: { step: data_processor }, path: "processed_data" }
{ $from: { step: user_lookup }, path: "user.profile.email" }
```

### Future Reference Types

```yaml
# Environment variables (planned)
{ $from: { env: "DATABASE_URL" } }

# Configuration values (planned)
{ $from: { config: "app.settings.timeout" } }

# Workflow metadata (planned)
{ $from: { workflow: metadata }, path: "version" }
```

## Path Expressions

### Dot Notation

Access nested object properties using dot notation:

```yaml
# Simple nested access
path: "user.name"
path: "result.data.summary"

# Mixed object and array access
path: "users[0].profile.email"
path: "results.items[2].metadata.timestamp"
```

### JSON Pointer Syntax

Use full JSON Pointer specification (RFC 6901):

```yaml
# JSON Pointer paths (alternative to dot notation)
path: "/user/name"
path: "/result/data/summary"
path: "/users/0/profile/email"

# Paths with special characters
path: "/field with spaces/data"
path: "/field~1with~0tildes/value"  # Escaping ~ and /

# Array index access
path: "/items/0"
path: "/nested/array/5/field"
```

### Array Operations

```yaml
# Array element access
path: "items[0]"          # First element
path: "items[-1]"         # Last element
path: "items[2]"          # Third element

# Nested array access
path: "data.records[0].values[1]"
path: "results.batches[0].items[-1].id"

# Multiple array dimensions
path: "matrix[0][1]"      # 2D array access
path: "data.grid[2][3].value"
```

### Path Edge Cases

```yaml
# Empty path (reference entire object)
path: ""                  # Same as omitting path

# Root array element
path: "[0]"              # First element of root array

# Fields with special characters
path: "field-with-dashes"
path: "field_with_underscores"
path: "field.with.dots"   # Accesses nested objects

# Numeric field names
path: "123"              # Field named "123"
path: "data.456.value"   # Nested numeric field name
```

## Skip Actions

### Skip Action Types

```yaml
# Skip the consuming step (default)
{
  $from: { step: optional_data },
  $on_skip: "skip_step"
}

# Use default value instead
{
  $from: { step: optional_data },
  $on_skip: "use_default",
  $default: { status: "no_data", value: null }
}

# Future: Continue with null
{
  $from: { step: optional_data },
  $on_skip: "use_null"  # Planned feature
}
```

### Default Value Types

```yaml
# Simple default values
$default: "fallback_string"
$default: 0
$default: false
$default: null

# Complex default values
$default:
  status: "unavailable"
  data: []
  metadata:
    source: "default"
    timestamp: null

# Nested expression defaults
$default:
  computed_field: { $from: { step: backup_computation } }
  static_field: { $literal: "backup_value" }
```

### Skip Propagation Patterns

```yaml
# Chain of optional dependencies
steps:
  - id: optional_step_1
    component: data://fetch
    skip_if: { $from: { workflow: input }, path: "skip_optional" }
    input:
      source: "external_api"

  - id: optional_step_2
    component: data://process
    input:
      data:
        $from: { step: optional_step_1 }
        $on_skip: "skip_step"  # Propagates skip

  - id: robust_step
    component: data://combine
    input:
      required_data: { $from: { step: required_step } }
      optional_data:
        $from: { step: optional_step_2 }
        $on_skip: "use_default"
        $default: { processed: false, data: [] }
```

## Complex Expression Patterns

### Conditional Data Selection

```yaml
# Select data based on conditions
input:
  data_source:
    $from: { step: check_environment }
    path: { $if: "is_production", then: "prod_data", else: "test_data" }

# Multiple fallback sources
input:
  user_data:
    # Try primary source first
    $from: { step: primary_lookup }
    $on_skip: "use_default"
    $default:
      # Fall back to secondary source
      $from: { step: secondary_lookup }
      $on_skip: "use_default"
      $default:
        # Final fallback to static data
        $literal: { status: "unknown", data: {} }
```

### Data Transformation Patterns

```yaml
# Extract and reshape data
input:
  user_summary:
    user_id: { $from: { step: user_data }, path: "id" }
    full_name: { $from: { step: user_data }, path: "profile.name" }
    email: { $from: { step: user_data }, path: "contact.email" }
    last_login:
      $from: { step: user_data }
      path: "activity.last_login"
      $on_skip: "use_default"
      $default: null

# Combine multiple data sources
input:
  enriched_record:
    base_data: { $from: { step: load_base_data } }
    enrichment:
      location: { $from: { step: geo_lookup }, path: "location" }
      demographics:
        $from: { step: demo_lookup }
        path: "demographics"
        $on_skip: "use_default"
        $default: {}
    metadata:
      processing_timestamp: { $literal: "2024-01-15T10:30:00Z" }
      version: { $from: { workflow: metadata }, path: "version" }
```

### Dynamic Field References

```yaml
# Dynamic path construction (future feature)
input:
  field_value:
    $from: { step: data_source }
    path: { $concat: ["results.", { $from: { workflow: input }, path: "field_name" }] }

# Conditional path selection
input:
  data:
    $from: { step: source_data }
    path:
      $if: { $from: { workflow: input }, path: "use_summary" }
      then: "summary.data"
      else: "full.data"
```

## Expression Validation

### Schema Validation for Expressions

```yaml
# Expression schema validation
input_schema:
  type: object
  properties:
    user_reference:
      # Validate expression structure
      type: object
      properties:
        $from:
          type: object
          properties:
            step:
              type: string
              pattern: "^[a-zA-Z][a-zA-Z0-9_]*$"
        path:
          type: string
          pattern: "^[a-zA-Z0-9._\\[\\]-]*$"
      required: ["$from"]
```

### Expression Linting

```yaml
# Lint rules for expressions (future tooling)
lint_rules:
  - rule: "no_undefined_steps"
    message: "Referenced step must be defined in workflow"

  - rule: "valid_json_pointer"
    message: "Path must be valid JSON pointer syntax"

  - rule: "no_circular_references"
    message: "Steps cannot reference themselves or create cycles"

  - rule: "required_default_values"
    message: "use_default actions must specify $default value"
```

## Advanced Expression Features

### Expression Composition

```yaml
# Nested expressions
input:
  computed_value:
    base_value: { $from: { step: base_computation } }
    multiplier:
      $from: { step: config_loader }
      path: "multipliers.default"
      $on_skip: "use_default"
      $default: 1.0
    metadata:
      source_step: { $literal: "base_computation" }
      fallback_used: { $literal: false }
```

### Expression Arrays

```yaml
# Arrays containing expressions
input:
  data_sources:
    - $from: { step: primary_source }
    - $from: { step: secondary_source }
      $on_skip: "use_default"
      $default: []
    - $literal: { source: "static", data: [] }

# Mixed literal and expression content
input:
  processing_config:
    - $literal: { type: "validation", enabled: true }
    - type: { $literal: "transformation" }
      config: { $from: { step: load_transform_config } }
    - $from: { step: load_output_config }
```

### Expression Functions (Future)

```yaml
# Planned expression functions
input:
  computed_fields:
    # String operations
    uppercase_name: { $upper: { $from: { step: user_data }, path: "name" } }
    formatted_email: { $lower: { $from: { step: user_data }, path: "email" } }

    # Math operations
    total_score:
      $add:
        - { $from: { step: score1 }, path: "value" }
        - { $from: { step: score2 }, path: "value" }

    # Date operations
    formatted_date:
      $date_format:
        date: { $from: { step: event_data }, path: "timestamp" }
        format: "YYYY-MM-DD"

    # Conditional operations
    status_message:
      $if:
        condition: { $gt: [{ $from: { step: score }, path: "value" }, 80] }
        then: "Excellent"
        else: "Good"
```

## Error Handling in Expressions

### Expression Evaluation Errors

```yaml
# Handle path evaluation errors
input:
  safe_field_access:
    $from: { step: data_source }
    path: "potentially.missing.field"
    $on_error: "use_default"
    $default: "field_not_found"

# Multiple error handling strategies
input:
  robust_computation:
    primary_value:
      $from: { step: primary_calc }
      $on_error: "use_fallback"
      $fallback:
        $from: { step: secondary_calc }
        $on_error: "use_default"
        $default: 0
```

### Type Coercion

```yaml
# Automatic type conversion (planned)
input:
  numeric_field:
    $to_number: { $from: { step: string_data }, path: "numeric_string" }

  string_field:
    $to_string: { $from: { step: numeric_data }, path: "number_value" }

  boolean_field:
    $to_boolean: { $from: { step: text_data }, path: "flag_text" }
```

## Performance Considerations

### Expression Optimization

```yaml
# Efficient expression patterns
input:
  # Good: Reference specific fields
  user_name: { $from: { step: user_data }, path: "name" }
  user_email: { $from: { step: user_data }, path: "email" }

  # Avoid: Multiple references to same large object
  # Instead use: Store in blob and reference blob_id

# Minimize expression complexity
input:
  # Good: Simple path reference
  result: { $from: { step: processor }, path: "output.result" }

  # Avoid: Deeply nested expressions (when possible)
  complex_result:
    deeply:
      nested:
        expression:
          $from: { step: complex_processor }
          path: "level1.level2.level3.level4.result"
```

### Expression Caching

```yaml
# Cache expensive expressions (future feature)
input:
  expensive_computation:
    $from: { step: heavy_processor }
    path: "result"
    $cache: true
    $cache_key: "heavy_result_v1"
```

## Debugging Expressions

### Expression Tracing

```yaml
# Enable expression debugging (future feature)
debug:
  trace_expressions: true
  expression_breakpoints:
    - step: "data_processor"
      path: "input.complex_reference"

# Expression logging
input:
  logged_reference:
    $from: { step: data_source }
    path: "important.field"
    $log: "Accessing critical field"
    $log_level: "debug"
```

### Expression Testing

```yaml
# Test specific expressions
test:
  expression_tests:
    - name: "test_user_reference"
      expression: { $from: { step: user_lookup }, path: "user.email" }
      mock_data:
        user_lookup:
          user:
            email: "test@example.com"
      expected: "test@example.com"

    - name: "test_skip_handling"
      expression:
        $from: { step: optional_data }
        $on_skip: "use_default"
        $default: "no_data"
      mock_data:
        optional_data: { $skip: true }
      expected: "no_data"
```

## Best Practices

### Expression Design

1. **Keep paths simple**: Use clear, straightforward path expressions
2. **Handle missing data**: Always consider what happens when data is unavailable
3. **Use appropriate defaults**: Provide meaningful fallback values
4. **Document complex expressions**: Add comments explaining complex reference patterns
5. **Test edge cases**: Include tests for missing data and error conditions

### Performance Guidelines

1. **Reference specific fields**: Avoid copying entire large objects
2. **Use blob storage**: Store large datasets in blobs and reference blob_ids
3. **Minimize expression nesting**: Keep expression trees shallow when possible
4. **Cache repeated computations**: Store expensive results for reuse

### Maintainability

1. **Use descriptive step names**: Make references self-documenting
2. **Group related expressions**: Organize complex input structures logically
3. **Consistent naming**: Use consistent field naming patterns across workflows
4. **Version expression patterns**: Document changes to expression structures

The expression system provides powerful data flow capabilities while maintaining clarity and performance. These patterns enable building robust, maintainable workflows that handle real-world data complexity gracefully.