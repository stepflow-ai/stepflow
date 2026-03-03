---
date: 2026-03-02
title: "Using Stepflow to Transforms Docling into a Scalable Document Processing Pipeline"
description: "Stepflow's orchestration architecture turns docling's document processing into a distributed, observable, and recoverable pipeline, allowing for additional observability, true horizontal scaling, and laying the groundwork for per-page fan-out"
slug: docling-step-worker-deploy
authors:
  - natemccall
tags: [production, kubernetes, docling]
draft: true
---

# How Stepflow Transforms Docling into a Scalable Document Processing Pipeline

[Docling](https://github.com/docling-project/docling) is a powerful document processing library. It handles PDF parsing, layout analysis, table extraction, OCR, and multi-format export. It's the kind of specialized AI pipeline that does one thing very well. But scaling it in production means running [docling-serve](https://github.com/docling-project/docling-serve), which brings its own set of architectural constraints: async task state pinned to a single process, no horizontal scaling path, and the common workaround of running it as a sidecar next to whatever system needs document processing.

Stepflow is a general purpose AI workflow system designed to solve exactly this class of problem. Its orchestration architecture provides resilient distributed execution state, HTTP SSE-aware load-balanced routing to worker pools, persistent result storage, and full observability out of the box. After initial success with scaling Langflow throughput, we started looking for other high-value integration projects. After looking through docling throughput as part of some day-job work with [OpenRAG](https://www.openr.ag/), we started to wonder: *how hard would it be to take a sophisticated AI pipeline like docling and run it entirely on Stepflow?*

The answer was a lot simpler than we though: some quick work shaping requests and responses for API parity via a simple proxy and setting up a basic Stepflow flow to define the workflow. This post walks through how Stepflow's architecture made that possible and what it means for scaling AI related pipeline tasks like document processing in production.

<!-- truncate -->

## What Stepflow Brings to the Table

Before diving into the Docling integration, it's worth understanding what Stepflow provides as an execution platform. These are the properties that make porting an AI pipeline straightforward rather than a months-long rewrite:

**Declarative flow definitions.** Workflows are YAML files with typed inputs, outputs, and step dependencies. A new pipeline is a new YAML file, not new application code.

**Load-balanced worker pools.** Stepflow's built-in load balancer routes executions to component-specific workers using least-connections balancing. Adding capacity means scaling a deployment, not re-architecting. Unit economics become a lot more manageable as we can right size instances to specifc types of tasks, like model execution or calculating embedding vs document parsing.

**Distributed execution state.** All run state lives in the orchestrator's storage backend (SQLite or external), not in worker memory. Any pod can serve status queries. Runs survive pod restarts, picking up where they left off. 

**Persistent result storage.** Flow outputs are stored in Stepflow's content-addressed blob store. Results are retrievable by ID from any pod that with the correct credentials. 

**Built-in observability.** Every component execution emits OpenTelemetry traces, metrics, and logs. The production deployment includes a full o11y stack: Jaeger for traces, Prometheus for metrics, Loki for logs, and Grafana dashboards.

**Recovery.** In-flight subflows are resumed during recovery. Execution checkpoints allow faster restart after failures.

These aren't docling-specific features. They're what Stepflow provides to *any* pipeline that plugs into it. Running Docling this way just happens to be a great example of why they matter!

## The Integration Architecture

The example in this post showcases a simple setup of three worker pods running docling. While lightweight enough to run on a laptop, nothing is stopping you from adding instances, turning up the CPU and scaling this out. That said, our initial deployment runs in the `stepflow-docling` namespace with three pods:

```mermaid
flowchart TB
    subgraph client["Client (localhost:5001)"]
        C["POST /v1/convert/source<br/>POST /v1/convert/file"]
    end

    subgraph orch["docling-orchestrator pod"]
        F["docling-facade<br/>(FastAPI, port 5001)"]
        S["stepflow-server<br/>(Rust, port 7840)"]
        F -->|"submit flow"| S
    end

    subgraph lb["docling-lb pod"]
        LB["Load Balancer<br/>(least-connections, port 8080)"]
    end

    subgraph workers["docling-worker pod(s)"]
        W1["Worker 1<br/>/classify, /convert, /chunk"]
        W2["Worker 2"]
        W3["Worker 3"]
    end

    subgraph o11y["stepflow-o11y namespace"]
        OT["OTel Collector"]
        J["Jaeger"]
        P["Prometheus"]
        G["Grafana"]
        OT --> J & P
        P --> G
    end

    C --> F
    S -->|"route /docling/*"| LB
    LB --> W1 & W2 & W3
    orch -.->|"traces/metrics"| OT
    workers -.->|"traces/metrics"| OT
```

For this achictecture, the facade is just a simple proxy. It accepts docling-serve's v1 HTTP API, translates the request into a Stepflow flow input, and submits it. Everything else, scheduling, routing, state management, result storage, observability, is handled by the Stepflow infrastructure that already exists.

The docling `DocumentConverter` library runs directly in the worker process using the Docling library directly for all funcitons. 

### The Flow Definition

The processing pipeline is defined as a standard Stepflow flow YAML:

```yaml
steps:
  - id: classify
    component: /docling/classify
    input:
      source: { $input: "source" }
      source_kind: { $input: "source_kind" }

  - id: convert
    component: /docling/convert
    input:
      source: { $input: "source" }
      source_kind: { $input: "source_kind" }
      pipeline_config: { $step: "classify", path: "recommended_config" }
      to_formats: { $input: "to_formats" }
      image_export_mode: { $input: "image_export_mode" }
      options: { $input: "options" }

  - id: chunk
    component: /docling/chunk
    input:
      document: { $step: "convert", path: "document_dict" }
      chunk_options: { $input: "chunk_options" }
```

This workflow uses three steps: classify, convert, chunk. Each step is a component that Stepflow routes to available workers through the load balancer. The flow is registered once at startup, then every document submission reuses the same flow definition with different inputs.

This is the same pattern that makes Stepflow's Langflow integration work. Define the pipeline declaratively and let the orchestrator handle execution. The difference is that here we're integrating a purpose-built document processing library rather than a visual workflow builder, but the integration surface is basically the same.

## Scaling: Load Balancers and Worker Pools

This is where Stepflow's architecture pays off most directly. docling-serve's scaling story has had some known limitations: async task state is stored in an in-memory dict, multiple Uvicorn workers see inconsistent state, and the project advises running with `UVICORN_WORKERS=1`.

With Stepflow, scaling is a deployment change:

```bash
kubectl scale deployment/docling-worker -n stepflow-docling --replicas=5
```

The load balancer discovers new worker pods automatically via the Kubernetes headless service as they are spun up. It uses least-connections routing, so work naturally flows to pods that have capacity. 

```mermaid
flowchart LR
    S["stepflow-server"] --> LB["Load Balancer"]
    LB -->|"2 active"| W1["Worker 1"]
    LB -->|"1 active"| W2["Worker 2"]
    LB -->|"0 active"| W3["Worker 3"]
    LB -->|"1 active"| W4["Worker 4"]
    LB -->|"0 active"| W5["Worker 5"]

    style W3 stroke:#2d8,stroke-width:2px
    style W5 stroke:#2d8,stroke-width:2px
```

Each worker pod runs the docling `DocumentConverter` with an LRU cache of converter instances keyed by options hash. First request to a pod takes ~30 seconds (model loading). Subsequent requests with the same options hit the cache and complete in ~10 seconds. As the worker pool warms up, the load balancer naturally routes new requests to warm pods.

### Performance Characteristics

From parity testing against the Docling technical paper (arXiv 2501.17887, ~37K characters of markdown output):

- **First conversion**: ~30s (model loading + LRU cache miss)
- **Subsequent conversions**: ~10s (LRU converter cache hit, same options hash)
- **File upload vs URL**: File upload slightly faster (no download step)

These timings are per-document. The scaling advantage comes from concurrent processing: while one worker is doing layout analysis on a 50-page PDF, other workers handle incoming requests. docling-serve can't do this because all state is pinned to the process that received the request. In the case of large batch submissions, Stepflow excels as the document is the unit of parallelism, and can therefore be spread accross an arbitrarily large pool of workers. 

## Observability: See Everything

Every component execution in Stepflow emits OpenTelemetry data. For the docling pipeline, this means you get distributed traces that span from the facade's HTTP handler through the orchestrator, load balancer, and into the worker's `DocumentConverter` call.

The deployment includes a complete observability stack in the `stepflow-o11y` namespace:

| Tool | URL | What It Shows |
|------|-----|---------------|
| Grafana | `localhost:3000` | Dashboards for throughput, latency, worker utilization |
| Jaeger | `localhost:16686` | Distributed traces across facade, orchestrator, and workers |
| Prometheus | `localhost:9090` | Metrics: request rates, processing times, queue depths |

This is particularly valuable for document processing where the cost of each request varies dramatically. A 5-page memo and a 500-page technical manual hit the same endpoint but have very different resource profiles. Traces let you see exactly where time is spent: PDF parsing, layout analysis (typically the bottleneck), table extraction, OCR, assembly.

docling-serve provides basic request logging. Stepflow gives you end-to-end distributed traces with per-step timing, automatically.

## Persistence and Recovery

Document processing is expensive. A 50-page PDF might take minutes of GPU/CPU time. Losing that work to a pod restart is not acceptable in production.

Stepflow's execution model handles this at two levels:

**Result persistence.** Flow outputs are stored in the content-addressed blob store. Once a conversion completes, the result is retrievable by ID from any pod. If the facade crashes after submitting a flow but before returning the response, the result still exists and can be retrieved.

**Execution recovery.** Stepflow checkpoints execution state. If the orchestrator restarts, in-flight subflows are resumed rather than restarted from scratch. A conversion that was 80% through layout analysis doesn't start over; it picks up from the last checkpoint.

This is infrastructure that docling-serve simply doesn't have. Async task state lives in a Python dict. Pod restart means lost results. Stepflow makes persistence the default, not an afterthought.

## What's Coming Next: Per-Page Fan-Out

The current flow processes documents sequentially through the classify, convert, and chunk steps. This is the correct starting point because it validates parity with docling-serve's behavior. But Stepflow's flow model is designed for exactly the kind of disaggregation that document processing needs.

The `convert` step is where most time is spent. Inside `DocumentConverter`, layout analysis runs per-page using `ThreadedStandardPdfPipeline`. That threading is limited by Python's GIL, which is why docling users report that setting `concurrency=10` only achieves ~2x speedup.

Stepflow's `/map` component provides a different approach: fan out pages across pods, not threads.

```mermaid
flowchart TB
    CL["classify"] --> SP["split pages"]
    SP --> M["/map"]

    subgraph fanout["Per-page fan-out across workers"]
        M --> P1["page 1<br/>Worker 1"]
        M --> P2["page 2<br/>Worker 2"]
        M --> P3["page 3<br/>Worker 3"]
        M --> P4["page N<br/>Worker ..."]
    end

    fanout --> A["assemble"] --> CH["chunk"]
```

Each page becomes an independent Stepflow flow execution, routed to available workers through the load balancer. Layout analysis, table extraction, and OCR for different pages run on different pods concurrently. No GIL contention. Linear scaling with worker count.

This isn't speculative; it's how Stepflow's `/map` component already works for Langflow batch processing. Applying it to document pages is the next integration step, and it happens behind the same facade. Callers still hit `POST /v1/convert/source` and get back the same response format.

## Building and Deploying

For this `stepflow-docling` namespace, we'll be building four images in order. All commands below are run from the `stepflow` repository root.

### 1. Build stepflow-server from source (~5 min)

Required because the published 0.9.0 images lack `runtimeProtocolVersion` in the initialize handshake, which stepflow-py 0.10.0 requires.

```bash
cd stepflow-rs
cargo build --release --target x86_64-unknown-linux-musl --bin stepflow-server

cp target/x86_64-unknown-linux-musl/release/stepflow-server release/
docker build -f release/Dockerfile.stepflow-server.alpine \
  -t localhost/stepflow-server:alpine-0.10.0 release/
```

### 2. Build docling-facade image (~10 sec)

Lightweight Python image with FastAPI, httpx, pyyaml. No docling library needed.

```bash
cd integrations/docling-step-worker
docker build --load -f docker/Dockerfile.facade \
  -t localhost/docling-facade:parity-v1 .
```

### 3. Build docling-worker image (~2 min)

Based on `docling-serve-cpu:v1.14.0` (4GB, includes pre-loaded models). Adds stepflow-py and the worker source.

```bash
cd integrations/docling-step-worker
docker build --load -f docker/Dockerfile \
  -t localhost/stepflow-docling-worker:parity-v1 .
```

### 4. Load into Kind and deploy

The load balancer uses the published image `ghcr.io/.../stepflow-load-balancer:alpine-0.9.0` directly. It's a transparent HTTP proxy that doesn't participate in protocol negotiation.

```bash
# Load images into Kind cluster
kind load docker-image localhost/stepflow-server:alpine-0.10.0 --name stepflow
kind load docker-image localhost/docling-facade:parity-v1 --name stepflow
kind load docker-image localhost/stepflow-docling-worker:parity-v1 --name stepflow

# Deploy (dependency-ordered: namespaces -> o11y -> worker -> LB -> orchestrator)
cd examples/production/k8s/stepflow-docling
./apply.sh
```

The apply script handles deployment ordering. The orchestrator pod includes an init container that waits for the worker and load balancer to be healthy before starting.

Verify:

```bash
kubectl get pods -n stepflow-docling
# Expected: orchestrator 2/2, worker 1/1, LB 1/1

kubectl get pods -n stepflow-o11y
# Expected: all o11y pods 1/1
```

## Three Deployment Blockers (and Their Fixes)

Getting the architecture right was the easy part. Getting all the pods healthy took a day of debugging three issues that exposed real integration boundaries.

### 1. Protocol version mismatch

The published stepflow-server 0.9.0 doesn't include `runtimeProtocolVersion` in the initialize handshake. The worker runs stepflow-py 0.10.0, which requires it. Worker pods CrashLoopBackOff'd with `Object missing required field: runtimeProtocolVersion`. Fix: build the server from source at 0.10.0 (the 40MB Alpine image shown above).

### 2. YAML quoting

Flow description values containing colons (`"Image reference mode (default: embedded)"`) parsed as YAML mapping keys when unquoted. The facade crashed on startup trying to load the flow definition. Fix: quote all description strings in the flow YAML and ConfigMap.

### 3. Missing optional flow inputs

Stepflow's `$input` references error on absent keys, even for logically optional fields. Minimal requests missing `image_export_mode` or `chunk_options` caused runtime failures. Fix: the facade's translator now always populates every field the flow references with sensible defaults:

```python
flow_input["to_formats"] = to_formats or ["md"]
flow_input["image_export_mode"] = image_export_mode or "embedded"
flow_input["options"] = opts if opts else None
flow_input["chunk_options"] = None
```

All three fixes landed in a single commit. The issues were integration-boundary problems, not architectural ones, which is exactly what you want when porting a new pipeline onto an orchestration platform.

## Parity Test Results

The real validation: does the facade behave identically to docling-serve from a caller's perspective? A six-test parity suite exercises the deployed system against the v1 API contract using the Docling technical paper from arXiv.

```bash
python test-parity.py --base-url http://localhost:5001 --timeout 300
```

| # | Test | What It Validates |
|---|------|-------------------|
| 1 | Health check | Facade + server reachable, retries for 60s |
| 2 | Convert URL source (v1) | `POST /v1/convert/source` with arXiv URL, verifies markdown output |
| 3 | Convert file upload (v1) | `POST /v1/convert/file` multipart upload of same PDF |
| 4 | v1alpha backward compat | `POST /v1alpha/convert/source` with `http_sources` shape |
| 5 | Options passthrough | `do_ocr: true, table_mode: accurate` accepted without error |
| 6 | Error handling | Invalid URL returns structured error response |

All six pass. Test 2 is the critical path: it exercises the full chain from facade through Stepflow server, load balancer, and worker, down to `DocumentConverter` and back.

## Try It Yourself

The complete deployment is at `examples/production/k8s/stepflow-docling/`. The CLAUDE.md in that directory covers Kind cluster setup, image builds, and teardown. The test script requires only Python 3.10+ and falls back to `urllib` if `httpx` isn't installed.

```bash
git clone https://github.com/stepflow-ai/stepflow.git
cd stepflow/examples/production/k8s/stepflow-docling
# See CLAUDE.md for Kind cluster setup and image builds
./apply.sh
python test-parity.py
./teardown.sh
```

The tracking issue is [#683](https://github.com/stepflow-ai/stepflow/issues/683) and the full design proposal is at `docs/proposals/docling-step-worker.md` in the repository.
