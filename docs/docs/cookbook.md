---
sidebar_position: 9
---

# Cookbook

This cookbook provides practical patterns and examples for common Stepflow use cases. Each pattern shows best practices and implementation approaches that have proven effective in real-world scenarios.

## Iteration and Loops

Stepflow includes built-in iteration components -- `iterate` and `map` -- but for more complex iteration patterns it is often better to implement custom iteration components.

The key to building these components yourself is to use the ability to evaluate a sub-flow asynchronously. This allows your component to launch a sub-workflow for each iteration with the component focusing on the iteration control logic.

```python title="Custom Iteration Component"
@server.component
async def iterate(input: IterateInput, context: StepflowContext) -> IterateOutput:
    """
    Iteratively apply a workflow until it returns a result instead of next.
    """
    current_input = input.initial_input
    iterations = 0

    while iterations < input.max_iterations:
        # Execute the workflow blob for this iteration
        result_value = await context.evaluate_flow_by_id(input.flow_id, current_input)
        iterations += 1

        # Check for termination condition
        if isinstance(result_value, dict) and "result" in result_value:
            return IterateOutput(
                result=result_value["result"],
                iterations=iterations,
                terminated=False,
            )

        # Continue iteration with new input
        elif isinstance(result_value, dict) and "next" in result_value:
            current_input = result_value["next"]
            continue

        # Invalid response format
        else:
            raise ValueError(f"Workflow must return {{'result': ...}} or {{'next': ...}}, got {result_value}")

    # Hit iteration limit
    return IterateOutput(
        result=current_input,
        iterations=iterations,
        terminated=True,
    )
```