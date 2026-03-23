---
sidebar_position: 100
hide_table_of_contents: true
---

import TOCInline from '@theme/TOCInline';

# FAQs

<TOCInline toc={toc} />

## What is Stepflow?

Stepflow is a workflow orchestrator for AI applications. You define workflows declaratively in YAML, and Stepflow handles execution, data flow, parallelism, and fault tolerance — while workers provide the actual business logic.

**What makes it different:**

- **Combine anything in one workflow.** A single Stepflow workflow can call an LLM, run a Python function, invoke an MCP tool, and execute a component from a completely different framework — each in its own worker process, with no shared runtime or dependency conflicts. You're not locked into one library's ecosystem.
- **Open protocol, any language.** Workers communicate with the orchestrator over an [open gRPC protocol](./protocol/index.md) using pull-based task queues. You can build workers in Python (using the [SDK](./workers/custom-components.md)), implement the protocol directly in any language, or use [MCP servers](./components/mcp-tools.md) as components with zero wrapping code.
- **Dev to production, no workflow changes.** During development, Stepflow runs as a [single binary](./getting-started.md) with embedded storage and subprocess workers. In production, the same workflows run on a [distributed cluster](./deployment/index.md) with dedicated worker pools, persistent storage, and message brokers — you change the infrastructure, not the workflow.
- **Production-grade by default.** Stepflow [journals every step result](./deployment/persistence-recovery.md), so workflows resume from the last successful step after a crash — no re-execution of expensive LLM calls. Workers run in isolated processes with independent scaling and [resource routing](./deployment/index.md) (GPU nodes for inference, standard compute for API calls).
- **Dynamic, composable flows.** Workflows can [spawn sub-workflows](./components/builtins/eval.md) at runtime via nested execution. The declarative format is simple enough for LLMs to author flows dynamically using a whitelisted set of components — enabling safe agentic patterns that most frameworks can't support.

Read on for detailed comparisons with [data processing technologies](#how-does-stepflow-compare-to-distributed-data-processing-technologies), [GenAI frameworks](#how-does-stepflow-compare-to-other-genai-workflow-systems), and [orchestration platforms](#how-does-stepflow-compare-to-other-orchestration-frameworks).

## How does Stepflow compare to distributed data processing technologies?

Technologies like Apache Spark, Flink, and Dask are designed for moving and transforming large datasets — they optimize for operations like shuffle, partitioning, and windowing across distributed compute clusters.

Stepflow solves a different problem. It orchestrates heterogeneous component executions — an LLM call, a file conversion, a tool invocation — where the data flowing between steps is typically modest-sized JSON rather than terabytes of records.

**Key differences:**

- **Component-centric, not data-centric.** Data processing frameworks operate on DataFrames and distributed collections. Stepflow wires together independent services via a declarative flow, where each component runs in its own process with its own runtime.
- **Orchestration vs. computation.** Stepflow manages sequencing, retries, and state persistence across services. Data processing frameworks manage computation over large datasets. These are complementary concerns — a Spark job can be a component within a Stepflow workflow.
- **Heterogeneous workloads.** A single Stepflow workflow might call an LLM, run a Python function, query a database, and invoke an MCP tool. Data processing frameworks typically execute homogeneous transformations across a dataset.

## How does Stepflow compare to other GenAI workflow systems?

Most GenAI workflow frameworks (agent frameworks, chain-based libraries, visual flow builders) are libraries where you compose chains or graphs in code. Stepflow takes a fundamentally different approach.

**Key differences:**

- **Protocol-based, not library-based.** Stepflow defines workflows declaratively in YAML/JSON and communicates with workers over an open protocol (gRPC with pull-based task queues). Components can be written in any language and deployed independently — they're not functions bound to a specific library's runtime. This means a single Stepflow workflow can combine components from different frameworks and ecosystems: one step might run a component built with one agent framework, the next a component from a completely different toolkit, each in its own worker — without either needing to know about the other.
- **Built for heterogeneous, dynamic workflows.** GenAI applications routinely involve different flows running at different points based on user context. Stepflow supports [nested flow execution](./components/builtins/eval.md) natively — a running workflow can spawn sub-workflows mid-execution via the [`eval` built-in](./components/builtins/eval.md). The declarative format also makes it straightforward for LLMs to author flows at runtime using a whitelisted set of allowed components, enabling safe agentic patterns that most frameworks don't support.
- **Orchestrator-worker separation.** In most GenAI frameworks, orchestration and component logic run in the same process. Stepflow [separates them](#why-does-stepflow-separate-the-orchestrator-from-workers) — gaining independent scaling, resource routing, security isolation, and fault tolerance. Workers pull tasks from the orchestrator (or from message brokers like NATS or Kafka), process them, and return results via gRPC.
- **Production durability built in.** Stepflow [journals every step result](./deployment/persistence-recovery.md), so workflows can resume from the last successful step after a crash. Most GenAI frameworks treat durability as an afterthought or external concern.

## How does Stepflow compare to other orchestration frameworks?

Established orchestration frameworks (DAG schedulers, task queues, workflow engines, cloud workflow services) each solve a specific coordination problem. Stepflow is designed specifically for AI workloads that combine LLMs, tools, and custom logic.

**Key differences:**

- **Designed for AI workloads, not batch ETL or task queues.** DAG schedulers are built for data pipelines with scheduled runs. Task queues distribute uniform work items. Cloud workflow services coordinate microservices with state machines. Stepflow is built for workflows where steps are heterogeneous service calls with structured data flow between them — including the ability to [execute nested workflows](./components/builtins/eval.md) dynamically.
- **Declarative flows with automatic parallelism.** Unlike imperative task chains or Python DAG definitions, Stepflow workflows are [declarative YAML](./flows/index.md) with automatic dependency analysis — steps without dependencies run in parallel with no explicit orchestration code.
- **Scales from a single binary to a distributed cluster.** In development, Stepflow runs as a self-contained local binary (also available as the [`stepflow-orchestrator` Python package](./getting-started.md)) with embedded storage and subprocess-based workers. In production, the same workflows run on a [cluster with separate storage, dedicated worker pools, and message brokers](./deployment/index.md) — no workflow changes required.
- **Open protocol vs. platform lock-in.** Cloud workflow services lock you into a vendor. Other frameworks require adopting their specific SDK and execution model. Stepflow's [protocol](./protocol/index.md) is open and language-agnostic: workers pull tasks via a standard queue interface, return results via gRPC, and workflows are portable across environments.

## Why does Stepflow separate the orchestrator from workers?

A core architectural decision in Stepflow is that the orchestrator and workers are separate processes connected by a protocol. Many of Stepflow's benefits flow directly from this separation:

- **Scalability.** The orchestrator coordinates workflow execution while workers do the actual computation. You can scale workers independently — add more GPU workers when inference load increases, scale down lightweight workers when idle — without touching the orchestrator.
- **Resource routing.** Different components can be [routed to different worker pools](./deployment/index.md) matched to their resource requirements. GPU-heavy inference runs on GPU nodes, memory-intensive processing on high-memory nodes, and lightweight API calls on standard compute — all within the same workflow.
- **Security and isolation.** Each worker runs in its own process (or container, or node). A component that processes sensitive data can run in an isolated environment with restricted network access. Components with conflicting dependencies — or from entirely different frameworks — run in separate workers without interference.
- **Fault tolerance.** A crash in one worker doesn't affect the orchestrator or other workers. The orchestrator [journals every step result](./deployment/persistence-recovery.md), so when a worker fails, the workflow resumes from the last successful step. Workers can be restarted, replaced, or rolled back independently.
- **Framework composition.** Because the protocol is the boundary — not a shared library — a single workflow can combine components built with different frameworks, languages, and runtimes. Each runs in its own worker, communicating through the same open protocol.

This separation is what enables Stepflow to [scale from a self-contained local binary to a distributed cluster](./deployment/index.md) without changing workflow definitions.

## What is the Stepflow Protocol, and why does it matter?

The Stepflow Protocol defines how the orchestrator and workers communicate. It's an open standard — not a proprietary SDK — so workers can be implemented in any language.

**Key aspects:**

- **Pull-based task distribution.** Workers pull tasks from the orchestrator (or from message brokers like NATS or Kafka) and return results via gRPC. Workers control their own concurrency by choosing when to pull the next task — no work is pushed onto an overloaded worker.
- **Bidirectional communication.** Tasks flow from orchestrator to workers via the task queue, while workers can make requests back to the orchestrator via gRPC — submitting nested workflows, querying run status, or storing blobs. This enables sophisticated patterns beyond simple request-response.
- **Schema-driven contracts.** Components declare their input/output schemas, enabling [validation](./cli/validate.md) before execution and tooling like auto-generated documentation.

Learn more in the [Protocol documentation](./protocol/index.md).

## Can I use Stepflow with my existing components and tools?

Yes. Stepflow is designed to integrate with existing tools rather than replace them.

- **Combine frameworks in a single workflow.** Because Stepflow communicates with workers over an open protocol, a single workflow can combine components from different frameworks and ecosystems. For example, an agentic flow could route one step to a worker running one agent framework and another step to a worker built with a completely different toolkit — each component runs in its own worker process, so framework dependencies never conflict.
- **MCP tools work directly.** Any [Model Context Protocol](https://modelcontextprotocol.io/) server can be used as a Stepflow component with zero wrapping code — just add it to your [configuration](./configuration.md).
- **Python SDK for custom components.** Wrap any Python function as a component with a few lines of code using the [Python SDK](./workers/custom-components.md). The SDK handles the pull-based task loop and gRPC communication automatically.
- **Open protocol for any language.** The gRPC interface is language-agnostic. If you have an existing service, you can implement the worker protocol directly or put a thin adapter in front of it.
- **Resource routing.** Different components can be routed to different worker pools based on their requirements — run ML models on GPU nodes, data processing on memory-optimized nodes, and lightweight API calls on standard compute, all within the same workflow.

## How does Stepflow handle loops and control flow?

Stepflow workflows are DAGs — steps with dependencies run in order, steps without dependencies run in parallel. But many real-world patterns require loops, iteration, or conditional branching that goes beyond a static DAG.

Stepflow solves this through **nested flow execution**: a component can submit sub-runs back to the orchestrator via [bidirectional callbacks](./protocol/task-lifecycle.md#bidirectional-callbacks), executing a flow definition as many times as needed. This works like higher-order functions — the component provides the looping or branching logic in native code (e.g., a Python `for` loop), while Stepflow defines the "body" of each iteration as a declarative flow.

**Built-in higher-order components:**

- [`iterate`](./components/builtins/iterate.md) — Repeatedly execute a flow until a termination condition is met, passing each iteration's output as the next iteration's input. Useful for convergence algorithms, state machines, and retry-until-success patterns.
- [`map`](./components/builtins/map.md) — Apply a flow to each item in a list, processing items in parallel. Useful for batch processing, data transformation, and fan-out patterns.

**Custom control flow:** You can write your own looping or branching components using the same mechanism. A custom component receives a flow definition as input, uses its native language's control structures (loops, conditionals, error handling), and submits sub-runs to the orchestrator for each execution of the flow body. This means any control flow pattern you can express in code can become a reusable Stepflow component. For example, here's the entire implementation of `iterate` in Python — a native `while` loop that delegates each iteration's body to the orchestrator:

```python
@server.component
async def iterate(input: IterateInput, context: StepflowContext) -> IterateOutput:
    current_input = input.initial_input
    iterations = 0

    while iterations < input.max_iterations:
        result_value = await context.evaluate_flow_by_id(input.flow_id, current_input)
        iterations += 1

        if isinstance(result_value, dict) and "result" in result_value:
            return IterateOutput(
                result=result_value["result"], iterations=iterations, terminated=False
            )
        elif isinstance(result_value, dict) and "next" in result_value:
            current_input = result_value["next"]
            continue
        else:
            raise StepflowValueError(
                "Iteration flow output must contain either 'result' or 'next' field"
            )

    return IterateOutput(result=current_input, iterations=iterations, terminated=True)
```

See the [loop components example](./examples/loop-components.mdx) for complete working examples including the [full source](https://github.com/stepflow-ai/stepflow/blob/main/examples/loop-components/loop_server.py).

## How does Stepflow handle failures and recovery?

Stepflow provides durability and fault tolerance at the orchestration layer, so individual components don't need to manage recovery logic.

- **Step-level journaling.** Every completed step result is [persisted](./deployment/persistence-recovery.md). If the orchestrator or a worker crashes, the workflow resumes from the last successful step — no re-execution of expensive LLM calls or API requests.
- **Worker isolation and routing.** The [orchestrator-worker separation](#why-does-stepflow-separate-the-orchestrator-from-workers) means a crash in one worker doesn't affect the orchestrator or other workers. [Routing rules](./configuration.md) direct different components to different worker pools, providing both resource management and fault isolation.
- **Configurable error handling.** Flows support per-step [error handling](./flows/control-flow.md) with fallback defaults, retry policies, and skip conditions — graceful degradation defined directly in the workflow definition.
