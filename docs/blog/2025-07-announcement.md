---
date: 2025-07-25
title: "Announcing StepFlow: An Open Protocol and Runtime for GenAI Workflows"
description: "Open Workflow Protocol and Runtime with JSON-RPC protocol for component servers."
slug: announcing-stepflow
authors:
  - benchambers
  - natemccall
tags: [announcements]
---

# Announcing StepFlow: An Open Protocol and Runtime for GenAI Workflows

We're excited to announce the initial release of StepFlow, an open-source
protocol and runtime designed to make building, executing, and scaling GenAI
workflows simple, secure, and portable. Whether you're prototyping locally or
deploying to production, StepFlow provides the foundation for reliable AI
workflow execution.

<!-- truncate -->

## Why StepFlow?

The rapid evolution of GenAI over the past couple of years has created an
explosion of tools, models, and APIs. While this diversity has powered some
serious innovation, it also creates challenges: How do you build workflows that
can leverage different AI services? How do you ensure your local prototypes will
scale in production? How do you maintain security and isolation when executing
untrusted code? How do you parallelise your workflows to maximise resource
utilization?

StepFlow was designed to fill these gaps for AI workflows, and address these challenges with a
simple yet powerful approach: define your workflows in YAML or JSON, and let the
runtime handle the complexity of execution, scaling, and security.

## Key Features in This First Release

### Reliable Workflow Execution for Everyone

StepFlow focuses on three core users: workflow creators, workflow frameworks,
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

GenAI in the modern enterprise requires workflow isolation for security and resource control. StepFlow builds this in at the
core:

- Process isolation: Each component runs in its own sandboxed process or node
- JSON-RPC protocol: Clean separation between workflow runtime and components
- Resource controls: Strict environment and resource management for each step

### Open and Extensible

Workflows are defined in JSON or YAML and submitted via a
wire protocol. This means that you are in complete control of all aspects of
runtime execution:

- Language agnostic: Write components in Python, TypeScript, or any language
- Well defined protocol and routing rules: Supports bidirectional communication with local or remote processes
- Plugin architecture: Easy integration with existing tools and services
- MCP support: Use Model Context Protocol tools as workflow components
- Lang Chain component Support: Use Lang Chain tools as workflow components
- Built-in components: OpenAI integration, file operations, and more out of the
  box
- User Defined Functions: Take control of your workflow via providing your own
  UDFs

## Geting Started

Our comprehensive [getting started](/docs/getting_started) guide and usage [examples](/docs/examples) should give you everything you need to get going with your own workflows. Specific examples include:

- Simple AI Chat Workflow: How to use [OpenAI chat integration](/docs/examples/ai-workflows)
- Data Processing Workflow: Build a custom pipeline to [process sales data](/docs/examples/data-processing)
- MCP tool integration: Use [MCP tools as workflow components](/docs/examples/mcp-tools)
- Custom component development: [Build your own components and UDFs](/docs/examples/custom-components)

## What's Next?

This is just the beginning! Our roadmap includes:

- Container-based components: Run components in Docker containers for ultimate
  isolation
- Distributed execution: Scale workflows across multiple machines
- Enhanced Python SDK: Simplified component development
- Richer component library: Pre-built components for common tasks
- Kubernetes integration: Native deployment to cloud environments

## Join the Community

StepFlow is open source and we welcome contributions! Whether you want to:

- Build new components
- Improve documentation
- Report bugs or suggest features
- Share your workflows

Visit our [GitHub repository](https://github.com/riptano/stepflow) to get
involved.

## Acknowledgments

Special thanks to all our early contributors and testers who helped shape
StepFlow. Your feedback and contributions have been invaluable in getting us to
this first release.

## Next Steps

Ready to build your first GenAI workflow? Check out our [documentation](/) and
join us in making AI workflows accessible, reliable, and secure for everyone.

**Happy workflow building!**

The StepFlow Team