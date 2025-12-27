---
sidebar_position: 3
---

# User-Defined Functions (UDFs)

User-Defined Functions (UDFs) allow you to execute custom Python code dynamically within Stepflow workflows. UDFs are particularly powerful because the code is stored as blobs and can be reused across multiple workflow steps, enabling flexible and maintainable data transformations.

:::note
Currently, UDFs are only supported in the Python component server.
However, the pattern described below should be possible in any language that supports dynamic code execution.
:::

## How UDFs Work

UDFs operate using a two-step process:

1. **Code Storage**: Python code and its input schema are stored as blobs using [`put_blob`](../builtins/put_blob.md)
2. **Code Execution**: The stored code is executed using `udf` with input data

This approach provides several advantages:
- **Reusability**: Same code blob can be used in multiple workflow steps
- **Maintainability**: Code changes only require updating the blob
- **Efficiency**: Code is compiled once and cached for subsequent executions
- **Type Safety**: Input schemas validate data before execution

## Simple UDF Example

Here's a basic example of creating and using a UDF:

```yaml
schema: https://stepflow.org/schemas/v1/flow.json

schemas:
  type: object
  properties:
    input:
      type: object
      properties:
        numbers:
          type: array
          items:
            type: number

steps:
# Store the UDF code as a blob
- id: create_average_udf
  component: /builtin/put_blob
  input:
    data:
      input_schema:
        type: object
        properties:
          numbers:
            type: array
            items:
              type: number
        required:
          - numbers
      code: |
        # Calculate the average of numbers
        numbers = input['numbers']
        if not numbers:
            return 0
        return sum(numbers) / len(numbers)

# Execute the UDF
- id: calculate_average
  component: /python/udf
  input:
    blob_id:
      { $step: create_average_udf, path: blob_id }
    input:
      numbers:
        { $input: "numbers" }

output:
  average:
    { $step: calculate_average }
```

:::note
UDFs have access to the same [context API](./custom-components.md#context) as custom components, allowing them to interact with workflow state, manage blobs, and access metadata.
:::

## Writing UDFs

UDFs can be written in several ways, depending on your needs. Here are some common patterns.

### Function Body
The most common approach is to provide the function body operating on the `input` dictionary directly and returning the result.

```python
numbers = input['numbers']
if not numbers:
  return 0
return sum(numbers) / len(numbers)
```

### Lambda
For simple cases, you can use a lambda that takes `input` to avoid the explicit `return`.

```python
lambda input: sum(input['values']) / len(input['values']) if input['values'] else 0
```

### Lambda with Context
Lambdas also allow you to take the optional `context` parameter to interact with workflow state or manage blobs:

```python
lambda input, context: context.put_blob(input['data']) if input.get('data') else None
```

### Named Function
You can define a complete function and reference it by name:

```python
def calculate_average(input):
    numbers = input['numbers']
    if not numbers:
        return 0
    return sum(numbers) / len(numbers)
calculate_average
```

:::tip
The function may also be `async` if it needs to perform asynchronous operations.
:::


### Named Function with Context
As with lambdas, this allows you to use the `context` parameter to manage workflow state or blobs:

```python
def process_items(input, context=None):
    """Process items and return summary statistics."""
    items = input['items']

    if not items:
        return {"count": 0, "summary": "No items to process"}

    # Extract numeric values if they exist
    values = []
    for item in items:
        if 'value' in item and isinstance(item['value'], (int, float)):
            values.append(item['value'])

    result = {
        "count": len(items),
        "numeric_count": len(values),
        "summary": f"Processed {len(items)} items"
    }

    if values:
        result.update({
            "sum": sum(values),
            "average": sum(values) / len(values),
            "min": min(values),
            "max": max(values)
        })

    return result

# Function name to execute
process_items
```

## UDF vs. Custom Component

The choice between using a UDF or a custom component depends on your specific use case:
- **UDFs** are ideal for dynamic code execution where the logic may change frequently and/or it makes sense to encapsulate the code in a specific flow.
- **Custom Components** are better suited for reusable libraries of functions that can be shared across multiple workflows, providing a more structured and maintainable approach.

Custom components offer slightly better performance and type safety, while UDFs provide more flexibility for dynamic code execution.

## Next Steps

- Learn more about [Custom Components](./custom-components.md) to create reusable component libraries
- Explore [Blob Storage](../builtins/put_blob.md) for advanced data management patterns
- Check out [Workflow Examples](../../examples/) that use UDFs for complex data processing