---
sidebar_position: 1
---

# Integrations

Stepflow integrates with popular GenAI workflow frameworks, allowing you to run existing workflows on Stepflow's distributed execution engine.

## Available Integrations

| Integration | Description | Status |
|-------------|-------------|--------|
| [Langflow](./langflow) | Execute Langflow workflows with automatic parallelization and distributed execution | ✅ Production Ready |
| [LangChain](./langchain) | Use LangChain components directly in Stepflow workflows | ✅ Production Ready |

## Why Run Workflows on Stepflow?

Running your existing workflows on Stepflow provides:

- **Automatic parallelization**: Independent steps execute concurrently
- **Distributed execution**: Route components to specialized workers (GPU, CPU, external services)
- **Observability**: Built-in tracing, metrics, and logging via OpenTelemetry
- **Scalability**: Horizontal scaling through Stepflow's worker architecture
- **Portability**: Run the same workflows locally or in Kubernetes

## Adding New Integrations

Stepflow's protocol and SDK make it straightforward to add support for other workflow systems. If you're interested in contributing an integration, see the [Contributing Guide](/docs/contributing).
