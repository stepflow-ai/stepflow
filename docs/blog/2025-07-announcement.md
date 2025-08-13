---
date: 2025-07-25
title: "Announcing Stepflow: An Open Protocol and Runtime for GenAI Workflows"
description: "Open Workflow Protocol and Runtime with JSON-RPC protocol for component servers."
slug: announcing-stepflow
authors:
  - benchambers
  - natemccall
tags: [announcements]
---

# Announcing Stepflow: An Open Protocol and Runtime for GenAI Workflows

We're excited to announce the initial release of Stepflow, an open-source
protocol and runtime designed to make building, executing, and scaling GenAI
workflows simple, secure, and portable. Whether you're prototyping locally or
deploying to production, Stepflow provides the foundation for reliable AI
workflow execution.

<!-- truncate -->

## Why Stepflow?

The rapid evolution of GenAI over the past couple of years has created an
explosion of tools, models, and APIs. While this diversity has powered some
serious innovation, it also creates challenges: How do you build workflows that
can leverage different AI services? How do you ensure your local prototypes will
scale in production? How do you maintain security and isolation when executing
untrusted code? How do you parallelise your workflows to maximise resource
utilization?

LangFlow - one of our other projects - meets (exceeds even!) most of these
requirements with both a visual authoring experience and a built-in API server
that turns every agent into an API endpoint. These endpoints can then be
integrated into applications built on any framework or stack, supporting all
major vector databases, LLMs, and a variety of other AI tools.

However, it became clear to us that a pure server implementation for running
“flows” was necessary to really scale and provide isolation for security and
resource consumption. Stepflow was designed to fill this gap, not only for
LangFlow but every other workflow system, and address these challenges with a
simple yet powerful approach: define your workflows in YAML or JSON, and let the
runtime handle the complexity of execution, scaling, and security.

## Key Features in This First Release

### Reliable Workflow Execution for Everyone

Stepflow focuses on three core users: workflow creators, workflow frameworks,
and workflow platforms. Workflow creators can use more components, run in more
places, and scale more reliably. Workflow frameworks can leverage all the
existing components without the need to deal with scale and durability. Workflow
platforms can run workflows from any supported framework. Specific features in
this release include:

- Parallel execution: Steps run concurrently when possible, maximizing
  performance
- Error handling: Distinguish between business logic failures and system errors
- State management: Built-in support for durable execution with SQLite backend

### Secure Component Isolation

GenAI in the modern enterprise requires workflow
isolation for security and resource control. Stepflow builds this in at the
core:

- Process isolation: Each component runs in its own sandboxed process or node
- JSON-RPC protocol: Clean separation between workflow runtime and components
- Resource controls: Strict environment and resource management for each step

### Open and Extensible

Workflows are defined in JSON or YAML and submitted via a
wire protocol. This means that you are in complete control of all aspects of
runtime execution:

- Language agnostic: Write components in Python, TypeScript, or any language
- Plugin architecture: Easy integration with existing tools and services
- MCP support: Use Model Context Protocol tools as workflow components
- Built-in components: OpenAI integration, file operations, and more out of the
  box
- User Defined Functions: Take control of your workflow via providing your own
  UDFs

## Geting Started

Get the latest release from GitHub https://github.com/riptano/stepflow/releases.

Here's a simple workflow to try to get started.

```yaml
# math-workflow.yaml
input_schema:
  type: object
  properties:
    numbers:
      type: array
      items:
        type: number

steps:
  - id: sum_numbers
    component: python://sum
    args:
      values: { $from: $input, path: numbers }

  - id: square_result
    component: python://square
    args:
      value: { $from: sum_numbers, path: result }

outputs:
  sum: { $from: sum_numbers, path: result }
  squared: { $from: square_result, path: result }
```

## What's Next?

This is just the beginning! Our roadmap includes:

- Container-based components: Run components in Docker containers for ultimate
  isolation
- Distributed execution: Scale workflows across multiple machines
- Enhanced Python SDK: Simplified component development
- Richer component library: Pre-built components for common tasks
- Kubernetes integration: Native deployment to cloud environments

## Join the Community

Stepflow is open source and we welcome contributions! Whether you want to:

- Build new components
- Improve documentation
- Report bugs or suggest features
- Share your workflows

Visit our [GitHub repository](https://github.com/riptano/stepflow) to get
involved.

## Acknowledgments

Special thanks to all our early contributors and testers who helped shape
Stepflow. Your feedback and contributions have been invaluable in getting us to
this first release.

## Next Steps

Ready to build your first GenAI workflow? Check out our [documentation](/) and
join us in making AI workflows accessible, reliable, and secure for everyone.

**Happy workflow building!**

The Stepflow Team