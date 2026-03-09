# Proposal: Backpressure for Stepflow Worker Pipeline

**Status:** Draft
**Authors:** Nate McCall
**Created:** March 2026
**Related:** `docling-step-worker.md` (facade architecture), Issue [#717](https://github.com/stepflow-ai/stepflow/issues/717) (custom metrics)

## Summary

Two targeted changes fix the concurrent load failure in the docling-step-worker deployment:

1. **Worker concurrency semaphore (Python SDK):** Workers reject when busy with a clean JSON-RPC error rather than timing out silently. The orchestrator already knows how to retry these.

2. **Retry budget tuning (orchestrator config):** The default retry budget (3 attempts, ~4s total) is calibrated for transient network faults. For CPU-bound workers doing 25-second conversions, it needs to be much larger. This is already a configurable field — no code changes required for the docling deployment case.

Together these give correct behavior: the orchestrator accepts every run immediately and executes it. When a step hits a busy worker, it retries with appropriate patience. The run naturally "waits" at the busy step until a worker frees up — the orchestrator is backpressured by the step result it needs before it can proceed. No queue management, no admission control, no new protocol states.

**Current state:** Workers time out under load instead of rejecting cleanly. The retry budget exhausts in 4 seconds against a 25-second conversion. Runs fail.

**Target state:** Workers reject cleanly with a retriable error code. The orchestrator retries with patience appropriate for the workload. Runs complete under burst load, slower but without failures.

## Motivation

### The Failure Mode

Under concurrent load (concurrency 3, 10 PDFs), all conversions fail:

```
[facade] [1/10] attention-is-all-you-need.pdf  FAIL
[facade] [2/10] bert.pdf                        FAIL
...
[facade] Batch complete: 0/10 succeeded, total wall time: 308.6s
```

Root cause: each docling conversion takes 20-30 seconds of CPU-intensive work. Workers accept every `component_execute` request regardless of whether they can process it. Under load, requests to busy workers fail at the transport level. The orchestrator retries 3 times with fibonacci backoff (1s + 1s + 2s = 4s total). A worker blocked for 25 seconds is still blocked when all retries are spent.

### Why Orchestrator-Level Queueing Is the Wrong Fix

An earlier version of this proposal introduced a dispatch semaphore in the orchestrator to gate when runs begin executing. This is architecturally incorrect for two reasons:

**Conditional execution.** A flow may only use a given worker conditionally — the condition may not be known until after an earlier step computes. There is no way to know at submission time which workers a run will need.

**Temporal mismatch.** A flow may run a subflow for an hour before needing a specific worker. Worker saturation *now* is irrelevant to whether that worker will be available when actually needed. Holding the run back based on current worker state would cause unnecessary delay with no benefit.

The orchestrator is already correctly designed: accept runs immediately, execute eagerly. The right place for backpressure is at the individual step execution boundary. When the orchestrator dispatches a component execution and the worker is busy, *that request* should wait — not the entire run submission. The run is naturally backpressured at the step: it cannot advance to later steps until it gets the result of the current one.

### The Two Actual Problems

**Problem 1 — Workers fail loudly instead of rejecting cleanly.** A busy worker should return a retriable error, not a connection timeout. The orchestrator knows how to retry JSON-RPC transport errors; it doesn't need a transport failure to trigger that path.

**Problem 2 — Retry budget mismatch.** Three retries over 4 seconds is correct for "network hiccup." It is wrong for "CPU-bound worker occupied for 25 seconds." These are different failure modes and need different budgets. The config already supports this — it just needs to be set correctly for the workload.

## Design

### Layer 1: Worker Concurrency Semaphore (Python SDK)

Add an `asyncio.Semaphore` to the Python SDK HTTP server that limits concurrent `component_execute` requests per worker process.

**Where:** `sdks/python/stepflow-py/src/stepflow_py/worker/http_server.py`

**Behavior:**
- `component_execute` requests acquire the semaphore with `acquire_nowait()`
- If the semaphore is full, return HTTP 503 with a JSON-RPC error body:
  ```json
  {
    "jsonrpc": "2.0",
    "id": "...",
    "error": {
      "code": -32300,
      "message": "Server at capacity (1/1 concurrent component executions). Adjust STEPFLOW_MAX_CONCURRENT_COMPONENTS to change this limit."
    }
  }
  ```
- `initialize`, `component_info`, and health check requests are NOT subject to the semaphore — they must always respond even under load
- Log a warning on rejection: `Rejected component_execute: at capacity (1/1). Consider increasing STEPFLOW_MAX_CONCURRENT_COMPONENTS or adding more workers.`

**Why error code -32300:** This falls in the transport error range (-32300 to -32399) defined in `stepflow-core/src/error_code.rs`. The orchestrator's step runner classifies transport errors as retriable. The worker is saying "I'm occupied, try again" and the existing retry path handles it correctly with no code changes.

**Why HTTP 503:** The orchestrator's HTTP client reads the response body for any HTTP status and parses it as JSON-RPC. The 503 signals infrastructure that this backend is temporarily unavailable; the JSON-RPC body gives the orchestrator the semantic error code it needs for retry classification.

**Configuration:**

| Source | Parameter | Default |
|--------|-----------|---------|
| Environment | `STEPFLOW_MAX_CONCURRENT_COMPONENTS` | 4 |
| `run_http_server()` kwarg | `max_concurrent_components` | 4 |

Default of 4 is appropriate for mixed workloads. CPU-bound workloads like docling should set this to 1. The error message names the environment variable explicitly so operators can tune without reading documentation.

**Protocol flow:**

```
Orchestrator                    LB (Pingora)                  Worker
     │                              │                           │
     │── component_execute ────────►│── proxy ─────────────────►│
     │                              │                           │── semaphore.acquire_nowait()
     │                              │                           │   FAIL (at capacity)
     │                              │◄── HTTP 503 + JSON-RPC ──│
     │◄── HTTP 503 ────────────────│                           │
     │                              │                           │
     │   is_transport_error(-32300) = true                      │
     │   retry_count < transport_max_retries                    │
     │   sleep(backoff.delay(retry_count))                      │
     │                              │                           │
     │── retry ────────────────────►│── proxy ─────────────────►│
     │                              │                           │── semaphore.acquire_nowait()
     │                              │                           │   OK (previous work finished)
     │                              │◄── HTTP 200 + result ────│
     │◄── result ──────────────────│                           │
```

### Layer 2: Retry Budget Configuration

`RetryConfig` in `stepflow-core/src/transport_retry.rs` is already a first-class configurable struct with two relevant fields:

```rust
pub struct RetryConfig {
    /// Maximum number of retries for transport errors (default: 3)
    pub transport_max_retries: u32,
    /// Backoff strategy: Constant, Exponential, or Fibonacci (default: Fibonacci 1s–10s)
    pub backoff: BackoffConfig,
}
```

The docling deployment needs a larger budget. The right values depend on the worst-case conversion time and the number of workers. For a 2-worker deployment with 60-second worst-case conversions, the orchestrator may need to retry for up to ~60 seconds before a worker frees up (worst case: both workers just started a 60s job when the step is dispatched). Set the budget to comfortably cover this:

```yaml
# examples/production/k8s/stepflow-docling/orchestrator/configmap.yaml
retry:
  transportMaxRetries: 60
  backoff:
    type: fibonacci
    minDelayMs: 2000
    maxDelayMs: 15000
```

With fibonacci backoff at 2s min / 15s max, retries occur at: 2s, 2s, 4s, 6s, 10s, 15s, 15s, 15s... The budget of 60 retries covers well over 10 minutes of sustained worker saturation before giving up. A run will not fail due to a busy worker unless the system is genuinely unable to process it within the run's timeout.

**This is a config change only.** No Rust code changes required. The docling deployment overrides the default in its configmap.

### Architecture Overview

```
Docling Client (compare-batch.py, curl, etc.)
         │
         │  POST /v1/convert/file
         │
         ▼
┌─────────────────────────────────────────────┐
│  Facade (FastAPI)                           │
│                                             │
│  _submit_and_respond():                     │
│    POST /api/v1/runs (wait=true)            │
│    Holds connection open until result       │
│    Returns result or 504 on timeout         │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────┐
│  Orchestrator (Rust, Axum)                  │
│                                             │
│  Accepts all runs immediately.              │
│  Executes eagerly: classify → convert →     │
│  chunk.                                     │
│                                             │
│  On transport_error(-32300):                │
│    retry up to transportMaxRetries (60)     │
│    with fibonacci backoff (2s–15s)          │
│                                             │
│  Run "waits" naturally at the busy step.    │
│  Cannot advance until step result arrives.  │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────┐
│  Load Balancer (Pingora)                    │
│  Least-connections routing (unchanged)      │
└──────────┬──────────────┬───────────────────┘
           │              │
           ▼              ▼
┌──────────────┐  ┌──────────────┐
│  Worker 1    │  │  Worker 2    │
│              │  │              │
│  Semaphore   │  │  Semaphore   │  ◄── Layer 1
│  (max: 1)    │  │  (max: 1)    │
│              │  │              │
│  If busy:    │  │  If busy:    │
│  503 +-32300 │  │  503 +-32300 │
│              │  │              │
│  docling     │  │  docling     │
│  conversion  │  │  conversion  │
│  (20-60s)    │  │  (20-60s)    │
└──────────────┘  └──────────────┘
```

### Autoscaling Signal

Two metrics together give a complete picture of worker pool pressure. The 503 rate drives autoscaling; pending tasks provides diagnostic depth.

#### 503 Rate — KEDA autoscale trigger (already available, no new code)

`stepflow_lb_requests_total` is already labeled by `backend`, `status`, and `method` and recorded in the LB's `logging()` hook for every request. Worker semaphore rejections appear as `status="503"`. A sustained non-zero 503 rate means workers are actively rejecting work right now — the right leading indicator for scaling.

Since the docling deployment has a dedicated LB instance, all 503s from that LB are docling worker rejections. No additional label scoping is needed. (Note: if multiple worker pools are ever colocated behind a single LB, a `pool_name` label would be needed to scope this query.)

**KEDA ScaledObject:**

```yaml
apiVersion: keda.sh/v1alpha1
kind: ScaledObject
metadata:
  name: docling-worker-scaler
  namespace: stepflow-docling
spec:
  scaleTargetRef:
    name: docling-worker
  minReplicaCount: 2
  maxReplicaCount: 8
  cooldownPeriod: 120
  triggers:
    - type: prometheus
      metadata:
        serverAddress: http://prometheus.stepflow-o11y.svc:9090
        metricName: stepflow_lb_worker_rejection_rate
        query: rate(stepflow_lb_requests_total{status="503"}[2m])
        threshold: "0.1"   # ~6 rejections/minute sustained → scale up
```

The threshold of 0.1 (roughly 6 rejections per minute) is a starting point. Calibrate once production data is available — document processing load varies dramatically between a 5-page memo and a 600-page image-heavy PDF.

#### Pending Tasks — diagnostic signal (new instrumentation)

A pending task is a step execution currently sleeping in retry backoff in the orchestrator — it has been dispatched, hit a busy worker, and is waiting to retry. This isn't directly observable from the LB, but it's useful for understanding queue depth and retry pressure beyond what 503 rate alone shows.

**Add to orchestrator step runner:**

```rust
// In stepflow-server/src/metrics.rs (or stepflow-execution):
pub static PENDING_COMPONENT_EXECUTIONS: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "stepflow_pending_component_executions",
        "Number of component executions currently waiting in transport retry backoff",
        &["component"]
    ).unwrap()
});
```

In the step runner's transport retry loop, increment the gauge when entering a retry sleep and decrement when the sleep completes (either retrying or giving up). The `component` label lets you see which step is the bottleneck — for docling, `/docling/convert` will dominate during saturation.

This metric is not wired to KEDA. It provides Grafana visibility into retry depth: a spike in `stepflow_pending_component_executions{component="/docling/convert"}` alongside a 503 rate spike confirms the system is under backpressure rather than experiencing unrelated failures.

## Files to Modify

### Python SDK (Layer 1)

| File | Change |
|------|--------|
| `sdks/python/stepflow-py/src/stepflow_py/worker/http_server.py` | Add `asyncio.Semaphore` to `_HttpServerContext`, guard `component_execute` with `acquire_nowait()`, add `max_concurrent_components` parameter to `run_http_server()` |

### Orchestrator (Layer 2 — config only)

| File | Change |
|------|--------|
| `examples/production/k8s/stepflow-docling/orchestrator/configmap.yaml` | Add `retry.transportMaxRetries: 60` and `retry.backoff` with `minDelayMs: 2000, maxDelayMs: 15000` |

### Metrics

| File | Change |
|------|--------|
| `stepflow-rs/crates/stepflow-server/src/metrics.rs` | Add `stepflow_pending_component_executions` GaugeVec (label: `component`) |
| `stepflow-rs/crates/stepflow-execution/src/step_runner.rs` | Increment/decrement `PENDING_COMPONENT_EXECUTIONS` gauge in transport retry loop |

### Deployment

| File | Change |
|------|--------|
| `examples/production/k8s/stepflow-docling/worker/deployment.yaml` | Add `STEPFLOW_MAX_CONCURRENT_COMPONENTS=1` |
| `examples/production/k8s/stepflow-docling/keda-scaledobject.yaml` | New file: KEDA ScaledObject targeting `stepflow_active_runs` |

## Verification

1. **Worker semaphore.** Port-forward directly to a single worker pod (bypassing LB). Submit 2 concurrent `component_execute` requests. First succeeds, second gets HTTP 503 with JSON-RPC error code -32300.

2. **Retry patience.** With `transportMaxRetries: 60` and 2 workers at `max_concurrent=1`, submit 3 concurrent documents. All 3 should complete — the third retries until a worker frees up. Observe retry log lines in the orchestrator.

3. **503 rate metric.** During batch processing under load, observe `rate(stepflow_lb_requests_total{status="503"}[2m])` in Prometheus. Verify it spikes when workers are saturated and returns to zero as workers free up.

4. **Pending component executions metric.** Under load, observe `stepflow_pending_component_executions{component="/docling/convert"}`. Verify it increments when retries are in-flight and drains as workers become available.

5. **KEDA autoscale.** Submit a batch large enough to sustain the 503 rate above threshold. Verify KEDA triggers scale-up. Verify new pods are discovered by the LB and begin receiving work.

6. **Backward compatibility.** Run with default config (no retry override, no semaphore). Behavior identical to current for non-docling workloads.

7. **End-to-end.** Run `compare-batch.py --concurrency 3 --skip-download`. Expect 10/10 success — documents process with retries under load rather than failing at retry exhaustion.

## Open Questions

1. **Retry budget upper bound vs. run timeout.** Currently `transportMaxRetries` is a fixed count. A cleaner model might be "retry until `run.timeout_secs` elapses" rather than a count. This would eliminate the need to tune the count separately from the timeout. Worth considering as a future `RetryConfig` enhancement.

2. **Distinguishing "busy" from "broken" in retries.** Error code -32300 is treated as a generic transport error. If we wanted different retry budgets for "worker busy" vs "worker crashed," we could use distinct sub-codes (-32301, -32302, etc. — already noted as a TODO in `step_runner.rs`). Not required for the immediate fix.

3. **KEDA threshold calibration.** The `threshold: 3` is a placeholder. What instrumentation should the first production deployment collect to calibrate this? Conversion duration histograms per document size bucket (page count from the classify step) would be most valuable.

## Non-Goals (This Proposal)

- **Stream-based pull architecture (NATS JetStream / Apache Iggy).** The right long-term direction for elastic scaling. Out of scope here — the retry-based approach is a targeted fix for the current architecture and does not preclude that evolution.
- **LB capacity-aware routing.** Unnecessary given correct retry semantics. Least-connections is sufficient.
- **Orchestrator admission control / dispatch semaphore.** Architecturally incorrect — see Motivation section.
- **Per-step retry budget differentiation.** Useful future work (see Open Question 2), not required now.
