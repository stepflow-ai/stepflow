---
sidebar_position: 1
---

# Flows Overview

Workflows are the core abstraction in StepFlow that describe sequences of operations and data flow between them. StepFlow is specifically designed to express GenAI and agentic workflows, providing powerful capabilities for orchestrating AI models, tools, and data processing tasks.

## What are Workflows?

A workflow in StepFlow is a declarative specification that defines:
- **Input requirements** - What data the workflow needs to start
- **Processing steps** - The sequence of operations to perform
- **Data flow** - How information moves between steps
- **Output structure** - What the workflow produces

Workflows are typically defined in YAML files and can be executed by the StepFlow runtime either locally or as a service.

## Core Concepts

### Steps
A workflow consists of a sequence of **steps**, each with:
- A unique identifier
- A component that provides the execution logic
- Input data derived from workflow inputs, previous steps, or literal values
- Optional error handling and skip conditions

Learn more: [Steps Guide](./steps.md)

### Components
Components provide the actual logic executed by steps. They can be:
- **Built-in components** - Provided by StepFlow for common operations
- **External components** - Implemented using the StepFlow protocol
- **MCP tools** - Model Context Protocol server tools

Learn more: [Components Documentation](../components/)

### Expressions
StepFlow's expression system enables dynamic data references and transformations:
- Reference workflow inputs and step outputs
- Extract specific fields from complex data
- Handle missing data with defaults
- Support conditional logic

Learn more: [Expressions Reference](./expressions.md)

## Workflow Capabilities

### Control Flow
- **Conditional execution** - Skip steps based on runtime conditions
- **Error handling** - Graceful failure recovery with fallbacks
- **Parallel execution** - Automatic parallelization of independent steps
- **Sub-workflows** - Compose complex workflows from simpler ones

Learn more: [Control Flow Patterns](./control-flow.md)

### Data Management
- **Input/Output schemas** - Define and validate data structures
- **Blob storage** - Efficient handling of large data objects
- **Data transformations** - Reshape and combine data between steps

Learn more: [Input/Output Management](./input_output.md)

### Development Support
- **Testing framework** - Built-in test case support
- **Schema validation** - Ensure data integrity
- **Performance optimization** - Guidelines for efficient workflows

Learn more: [Testing Workflows](./testing.md) | [Performance Guide](./performance.md)

## Documentation Structure

### Getting Started
1. [Workflow Specification](./specification.md) - Complete reference for workflow syntax
2. [Steps](./steps.md) - Understanding and configuring workflow steps
3. [Expressions](./expressions.md) - Data references and transformations

### Core Features
4. [Control Flow](./control-flow.md) - Conditional execution and error handling
5. [Input/Output](./input_output.md) - Managing workflow data
6. [Schema Validation](../reference/flow-schema.mdx) - Ensuring data integrity

### Advanced Topics
7. [Execution Model](./execution.md) - How workflows run
8. [Testing](./testing.md) - Writing and running tests
9. [Performance](./performance.md) - Optimization guidelines
10. [Best Practices](./best-practices.md) - Patterns and recommendations

## Next Steps

- **New to StepFlow?** Start with the [Getting Started Guide](../getting_started.md)
- **Ready to build?** Check out [Workflow Examples](../examples/)
- **Need components?** Explore [Built-in Components](../components/builtins/index.md)
- **Building integrations?** Learn about the [StepFlow Protocol](../protocol/)