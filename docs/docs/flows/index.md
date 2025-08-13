---
sidebar_position: 1
---

# Flows Overview

Flows are the core abstraction in Stepflow.
Some of the most important parts of a flow are:

- **Input** - The schema for the input to the flow
- **Steps** - The steps to perform for the flow
- **Output** - The output the flow should produce

The steps and output are able to reference the input and the results of previous steps.
This creates data dependencies between steps, determining which steps must run in sequence and which can run in parallel.

Flows are typically defined in YAML files and can be executed by the Stepflow runtime either locally or as a service.

:::note
Flows conform to a [JSON schema](../reference/flow-schema.mdx) that defines the structure and requirements for flow definitions.
:::

## Input and Output

A flow defines an **input schema** that describes the expected structure of input data and an **output** describing how to create the output.
There is also an optional **output schema** that describes the structure of the output data.

Like steps, the output description uses [Expressions](#expressions) to reference input data and step outputs.

Learn more about [Input and Output](./input-output.md).

## Steps
A flow consists of a sequence of **steps**, each with:
- A unique identifier
- A component that provides the execution logic
- Input data consisting of flow inputs, previous steps, or literal values
- Optional error handling and skip conditions

Learn more about [Steps](./steps.md).

## Components
Components provide the actual logic executed by steps. They can be:
- **Built-in components** - Provided by Stepflow for common operations
- **External components** - Implemented using the Stepflow protocol
- **MCP tools** - Model Context Protocol server tools

Learn more about [Components](../components/index.md).

## Expressions {#expressions}
Stepflow's expression system enables dynamic data references and transformations:
- Reference flow inputs and step outputs
- Extract specific fields from complex data
- Handle missing data with defaults
- Support conditional logic

Learn more about [Expressions](./expressions.md)

## Control Flow

Control flow is determined by dependencies between steps.
By default, if a step is skipped or fails to execute, subsequent steps that depend on it will also be skipped.
However, you can define custom error handling and fallback logic to control how failures are managed.

Learn more about [Control Flow](./control-flow.md).

## Metadata

Flows and steps may contain additional metadata that is used in Stepflow or by external tools. Currently, this includes:

- `name`: A human-readable name for the flow
- `description`: A description of the flow's purpose
- `version`: The version of the flow definition

These will likely be extended in the future to allow an arbitrary `metadata` dictionary on the flow, steps, or both.

## Next Steps

- **New to Stepflow?** Start with the [Getting Started Guide](../getting-started.md)
- **Ready to build?** Check out [Workflow Examples](../examples/)
- **Need components?** Explore [Built-in Components](../components/builtins/index.md)
- **Building integrations?** Learn about the [Stepflow Protocol](../protocol/)