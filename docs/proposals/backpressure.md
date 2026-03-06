# Proposal: Backpressure for Stepflow Worker Pipeline

**Status:** Draft
**Authors:** Nate McCall
**Created:** March 2026
**Related:** `docling-step-worker.md` (facade architecture), Issue [#717](https://github.com/stepflow-ai/stepflow/issues/717) (custom metrics)

## Summary

This proposal adds backpressure to the Stepflow worker pipeline via two coordinated mechanisms: a **concurrency semaphore** in the Python SDK worker (Layer 1) that makes busy workers reject cleanly rather than fail silently, and a **dispatch semaphore** in the Rust orchestrator (Layer 2) that queues accepted runs rather than dispatching all of them immediately. Together they replace the current failure mode — transport errors, exhausted retries, cascading failures — with graceful queuing. Runs are durably accepted the moment they arrive. They wait for worker capacity to become available, then execute. Clients never see a new error shape.

A `stepflow_pending_runs` Prometheus gauge, emitted as a natural byproduct of Layer 2, provides the autoscale signal for Kubernetes HPA/KEDA. New worker pods join the pool immediately and begin draining the queue via the existing least-connections routing.

**Current state:** Unbounded. All requests accepted and dispatched immediately. Workers saturate. Transport retries (3 attempts, ~4s total) exhaust before a 25-second conversion finishes. Runs fail.

**Target state:** Workers reject when busy (clean 503 + JSON-RPC error, retried by orchestrator). Orchestrator queues runs that can't dispatch immediately. Pending run depth signals autoscaling. New workers drain the queue automatically. Docling clients see no new error shapes.

## Motivation

### The Failure Mode

Under concurrent load (concurrency 3, 10 PDFs), with less than 3 workers, all conversions fail:

```
[facade] [1/10] attention-is-all-you-need.pdf  FAIL  ({"detail": "step \"convert\" failed"...})
[facade] [2/10] bert.pdf                        FAIL  ({"detail": "step \"convert\" failed"...})
...
[facade] Batch complete: 0/10 succeeded, total wall time: 308.6s
```

Root cause: each docling conversion takes 20-30 seconds of CPU-intensive work. With concurrency 3, the facade submits 3 flow runs simultaneously. The orchestrator dispatches all component executions immediately — there is no gating. Workers accept every request but can only process one conversion at a time. Requests to busy workers fail at the HTTP transport level. The orchestrator retries 3 times with fibonacci backoff (1s + 1s + 2s = 4s total). A worker blocked for 25 seconds will still be blocked when all 3 retries have been spent. The run fails. There are settings within the docling runtime to increase parallelism (more details below), but this will impact throughput. We are also leaving these as is due to it presenting a real-world issue that, given our architecture, we should be able to easily handle. 

Currently there is no backpressure anywhere in the docling-step-worker stack:

| Layer | What exists | Gap |
|-------|------------|-----|
| Orchestrator (`max_in_flight=10`) | Limits concurrent *tasks* within a single run | No limit on concurrent *runs* or dispatch rate |
| LB (Pingora) | Least-connections routing, health checks every 10s | No per-backend limit, no rejection |
| Python worker (uvicorn) | 3 worker processes, `backlog=128` | No semaphore, no rejection, accepts everything |
| HTTP client (reqwest) | Fibonacci retry (3 attempts, ~4s) | Retry budget calibrated for transient network faults, not 25s CPU saturation |

### Two Separable Problems

**Problem 1 — Retry budget mismatch.** The retry budget (4s total) was designed for transient network issues. It is not appropriate for a worker that is legitimately busy for 25 seconds. The worker isn't broken; it's occupied. The orchestrator should keep retrying until its run timeout, not give up after 4 seconds.

**Problem 2 — Unbounded concurrent dispatch.** All submitted runs are live and dispatching immediately. Under burst load, every worker sees a flood of component executions simultaneously. Even with a fixed retry budget, submissions racing to the same saturated workers will collide repeatedly.

Fixing Problem 1 alone (extended retries) would solve the concurrency-3 failure case — the orchestrator would eventually hit a worker between its 25s jobs. But it leaves the system fragile under higher concurrency and does nothing for operator visibility or autoscaling. Problem 2 is the right fix, and solving it properly also addresses Problem 1 as a side effect.

### What Stock docling-serve Does

docling-serve uses an in-process thread pool (`DOCLING_SERVE_ENG_LOC_NUM_WORKERS`) to bound concurrency, a page queue (`DOCLING_SERVE_QUEUE_MAX_SIZE`) to absorb bursts, and `DOCLING_SERVE_MAX_SYNC_WAIT` (default 120s) to return HTTP 504 when a sync request can't complete in time. Clients expect either a successful response or 504. They do not handle 429.

Our architecture replaces docling-serve's in-process queue with Stepflow's orchestration layer. This is architecturally superior (durable, observable, multi-worker) but requires us to provide equivalent queueing behavior. The key constraint: **existing docling clients must not see new error shapes.**

### Design Principles

1. **Accept first, queue second, dispatch when ready.** The run is durably recorded the moment it arrives. Dispatch waits for capacity. Clients are never told "try again later."
2. **Workers should reject, not fail.** A busy worker should return a clean error that the orchestrator understands as retriable — not accept the request and produce a connection timeout.
3. **Pending depth is the right autoscale signal.** It directly represents "work waiting for capacity" — actionable and easy to reason about.
4. **No new client-visible error shapes.** Docling clients see 504 on timeout if queuing takes too long. They never see 429 or new error formats.

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
- `initialize`, `component_info`, and health check requests are NOT subject to the semaphore — they must always respond, even under load
- Log a warning on rejection: `Rejected component_execute: at capacity (1/1). Consider increasing STEPFLOW_MAX_CONCURRENT_COMPONENTS or adding more workers.`

**Why error code -32300:** This falls in the transport error range (-32300 to -32399) defined in `stepflow-core/src/error_code.rs:120-122`. The orchestrator's `flow_executor.rs:970-985` classifies transport errors as retriable, triggering automatic retry with fibonacci backoff. The worker is saying "I'm busy, try again" and the orchestrator handles it without any code changes to the retry path.

**Why HTTP 503:** The orchestrator's HTTP client (`protocol/src/http/client.rs:206-218`) reads the response body for any HTTP status and parses it as JSON-RPC. Returning 503 signals the LB and infrastructure that this backend is temporarily unavailable, while the JSON-RPC body gives the orchestrator the semantic error code it needs for retry classification.

**Configuration:**

| Source | Parameter | Default |
|--------|-----------|---------|
| Environment | `STEPFLOW_MAX_CONCURRENT_COMPONENTS` | 4 |
| `run_http_server()` kwarg | `max_concurrent_components` | 4 |

Default of 4 is appropriate for mixed workloads. CPU-bound workloads like docling should override to 1 (set via deployment config). The error message explicitly names the environment variable so operators can tune without reading documentation.

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
     │   retry after fibonacci delay                            │
     │                              │                           │
     │── retry ────────────────────►│── proxy ─────────────────►│
     │                              │                           │── semaphore.acquire_nowait()
     │                              │                           │   OK (previous work finished)
     │                              │◄── HTTP 200 + result ────│
     │◄── result ──────────────────│                           │
```

### Layer 2: Orchestrator Dispatch Semaphore (Rust)

Add a bounded semaphore that gates the point at which a submitted run begins execution. Runs are accepted and durably recorded immediately. Dispatch is throttled by the semaphore. Pending runs wait in memory (held by the blocked `submit_run` future on the Axum task) until a slot opens.

**Where:** `stepflow-rs/crates/stepflow-execution/src/executor.rs` — `submit_run()` function

**Key insight from the existing code:** `submit_run()` already does three things in sequence: (1) journal write, (2) metadata create, (3) `flow_executor.spawn()`. We insert the semaphore acquire between steps 2 and 3. The run is durable before it queues. Dispatch waits for capacity.

**Implementation:**

```rust
// In StepflowEnvironment (or ExecutionConfig):
pub struct DispatchSemaphore(Arc<Semaphore>);

// In submit_run(), after metadata create and before spawn:

// Track this run as pending and increment the gauge
PENDING_RUNS.inc();
let _permit = env.dispatch_semaphore().acquire_owned().await
    .change_context(ExecutionError::Shutdown)?;
PENDING_RUNS.dec();

// Permit moves into the spawned future — dropped when run completes,
// releasing the slot for the next pending run.
flow_executor.spawn_with_permit(env.active_executions(), _permit);
```

`spawn_with_permit` is a thin wrapper around the existing `spawn` that holds the permit for the lifetime of the executor task.

**Why this approach fits the existing model:**

- No new `ExecutionStatus` variants. The run is `Running` from the moment it's durable — that's semantically correct because it has been accepted and will execute. "Pending dispatch" is an internal execution detail, not a user-facing state.
- No changes to the API or response shapes. The facade's `wait=true` connection simply holds longer. `wait_for_completion()` already handles arbitrary durations via the broadcast channel notification system.
- No retry loop in the facade. The facade submits once and waits. The orchestrator manages queueing internally.
- Shutdown safety: `acquire_owned()` returns `Err` if the semaphore is closed, allowing clean shutdown of pending runs.

**Semaphore bound:** `max_concurrent_runs = 2 × worker_count` as a starting point. Headroom above worker count is intentional — `classify` and `chunk` steps are fast, so while `convert` occupies all workers, the next batch's `classify` can be in-flight. Configurable via `LimitsConfig` (see below).

**Configuration:**

| Source | Parameter | Default |
|--------|-----------|---------|
| `stepflow-config.yml` | `limits.maxConcurrentRuns` | unbounded |
| Environment | `STEPFLOW_MAX_CONCURRENT_RUNS` | unbounded |

Default is unbounded for backward compatibility. No behavioral change unless the limit is explicitly set.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LimitsConfig {
    /// Maximum concurrent flow runs dispatched simultaneously.
    /// Runs submitted beyond this limit are durably accepted and queued.
    /// None = unbounded (default).
    pub max_concurrent_runs: Option<usize>,
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self { max_concurrent_runs: None }
    }
}
```

### Pending Runs Metric and Autoscaling

The `PENDING_RUNS` gauge is a natural byproduct of the dispatch semaphore — it's the count of runs that have been accepted and journaled but are waiting for a dispatch slot.

**Metric definition:**

```rust
// In stepflow-server/src/metrics.rs (or stepflow-execution):
pub static PENDING_RUNS: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "stepflow_pending_runs",
        "Number of runs accepted and queued, waiting for a dispatch slot"
    ).unwrap()
});
```

**KEDA ScaledObject (initial configuration):**

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
  cooldownPeriod: 120       # seconds before scaling down
  triggers:
    - type: prometheus
      metadata:
        serverAddress: http://prometheus.stepflow-o11y.svc:9090
        metricName: stepflow_pending_runs
        threshold: "3"      # scale up when pending > 3
        query: stepflow_pending_runs
        # KEDA stabilization window handles the "for 10 seconds" requirement
```

The threshold of 3 and the 10-second stabilization window are initial guesses. Document processing load varies dramatically — a 5-page memo and a 600-page image-heavy PDF hit the same endpoint but have entirely different resource profiles. These values need calibration from runtime data. The numbers are deliberately conservative to start: scale early, scale back slowly.

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
│    Holds connection open                    │
│    Returns result or 504 on timeout         │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────┐
│  Orchestrator (Rust, Axum)                  │
│                                             │
│  create_run():                              │
│    → journal write (durable)                │
│    → metadata create (status=Running)       │
│    → PENDING_RUNS.inc()                     │
│    → dispatch_semaphore.acquire() ◄── waits │
│    → PENDING_RUNS.dec()                     │
│    → flow_executor.spawn_with_permit()      │
│                                             │
│  FlowExecutor dispatches steps:             │
│    classify → convert → chunk               │
│    on transport_error(-32300): retry        │
│    (retry budget: run timeout, not 3 fixed) │
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
│  Worker 1    │  │  Worker 2    │  ◄─── Layer 1
│  Semaphore   │  │  Semaphore   │
│  (max: 1)    │  │  (max: 1)    │
│              │  │              │
│  If busy:    │  │  If busy:    │
│  503 + -32300│  │  503 + -32300│
└──────────────┘  └──────────────┘
         │
         │  stepflow_pending_runs gauge
         ▼
┌─────────────────────────────────────────────┐
│  KEDA ScaledObject                          │
│  pending_runs > 3 for 10s → scale up        │
│  pending_runs == 0 → scale down (120s cool) │
└──────────────────┬──────────────────────────┘
                   │
                   ▼
         New worker pods join pool,
         LB discovers via headless service,
         dispatch semaphore routes next
         pending run to available capacity
```

### Retry Budget Adjustment

Layer 1 makes workers reject cleanly with -32300. The orchestrator's existing transport retry path picks this up correctly. However, the current fixed budget (3 attempts, ~4s) is calibrated for transient network faults, not occupied workers.

With Layer 2 in place, the dispatch semaphore prevents most cases where retries would be needed — a run only dispatches when a slot is available. Layer 1's 503 becomes a safety valve for the gap between semaphore acquire and a worker actually being free (e.g., a conversion finishes just as the semaphore releases and two runs race to the same worker).

For this safety-valve case, retries should be bounded by run timeout rather than a fixed count. This is a follow-on adjustment to the transport retry configuration, not a prerequisite for the initial implementation.

## Files to Modify

### Python SDK (Layer 1)

| File | Change |
|------|--------|
| `sdks/python/stepflow-py/src/stepflow_py/worker/http_server.py` | Add `asyncio.Semaphore` to `_HttpServerContext`, guard `component_execute` with `acquire_nowait()`, add `max_concurrent_components` parameter to `run_http_server()` |

### Rust Orchestrator (Layer 2)

| File | Change |
|------|--------|
| `stepflow-rs/crates/stepflow-execution/src/executor.rs` | Add dispatch semaphore acquire between metadata create and `flow_executor.spawn()` in `submit_run()`; emit `PENDING_RUNS` gauge |
| `stepflow-rs/crates/stepflow-plugin/src/lib.rs` | Add `DispatchSemaphore` to `StepflowEnvironment`; wire from `LimitsConfig` |
| `stepflow-rs/crates/stepflow-config/src/lib.rs` | Add `LimitsConfig` struct, add `limits` field to `StepflowConfig` |
| `stepflow-rs/crates/stepflow-server/src/metrics.rs` | Register `stepflow_pending_runs` gauge |

### Deployment

| File | Change |
|------|--------|
| `examples/production/k8s/stepflow-docling/worker/deployment.yaml` | Add `STEPFLOW_MAX_CONCURRENT_COMPONENTS=1` |
| `examples/production/k8s/stepflow-docling/orchestrator/configmap.yaml` | Add `limits.maxConcurrentRuns` (set to `2 × initial_worker_count`) |
| `examples/production/k8s/stepflow-docling/keda-scaledobject.yaml` | New file: KEDA ScaledObject targeting `stepflow_pending_runs` |

## Verification

1. **Worker semaphore.** Port-forward directly to a single worker pod (bypassing LB). Submit 2 concurrent `component_execute` requests. First succeeds, second gets HTTP 503 with JSON-RPC error code -32300.

2. **Dispatch semaphore queuing.** Set `maxConcurrentRuns: 2`. Submit 4 concurrent runs via the API. First 2 dispatch immediately. Runs 3 and 4 show `status: Running` in the metadata store but their FlowExecutors are not yet spawned. `stepflow_pending_runs` gauge reads 2. When run 1 or 2 completes, one pending run dispatches and the gauge decrements.

3. **PENDING_RUNS metric.** Under load, observe `stepflow_pending_runs` in Grafana/Prometheus. Verify it increments when the semaphore is contended and decrements as runs complete.

4. **Facade behavior unchanged.** Submit via `compare-batch.py --concurrency 3`. Facade submits with `wait=true` and holds the connection. No 429s observed. Documents eventually complete (slower under queueing, but all succeed).

5. **KEDA autoscale.** With `maxConcurrentRuns` set to match worker count, submit a batch that saturates. Verify `stepflow_pending_runs > 3` triggers KEDA to add worker pods. Verify new pods are discovered by the LB and the pending gauge drains.

6. **Backward compatibility.** Run with no `limits` config. Behavior identical to current — no semaphore, no queueing, no regressions.

## Open Questions

1. **Retry budget upper bound.** Should the orchestrator's transport retry limit be tied to `run.timeout_secs` when Layer 1 semaphore errors are detected? Or is the dispatch semaphore sufficient to make this a rare path, leaving it as a future refinement?

2. **`spawn_with_permit` interface.** `FlowExecutor::spawn` currently takes only `active_executions`. What's the cleanest way to thread the owned semaphore permit through — store it in `FlowExecutorBuilder`, pass it to `spawn`, or a separate `spawn_with_permit` variant?

3. **Pending run persistence.** Pending runs are queued in memory (blocked futures on Axum tasks). If the orchestrator restarts under load, pending runs are lost. The run is durable in the journal, but the client's open `wait=true` connection is gone. Is this acceptable for v1, or should we persist the pending queue? (Note: runs that were dispatched before the crash already benefit from existing recovery logic.)

4. **KEDA threshold calibration.** The `threshold: 3` and 10-second stabilization window are initial guesses. What instrumentation should we add to the first deployment to collect the data needed to tune these? Token/chunk metrics from the existing observability stack are a starting point.

5. **Scale-down aggressiveness.** The 120-second cooldown prevents thrashing but may leave over-provisioned workers running longer than necessary. Is this acceptable, or should the cooldown be tuned tighter once we have data on how quickly load subsides?

## Non-Goals (This Proposal)

- **Stream-based pull architecture (NATS JetStream / Apache Iggy).** This is the right long-term direction for truly elastic scaling, but it is a larger architectural shift. The dispatch semaphore approach described here is a targeted fix for the current architecture and does not preclude that future evolution.
- **LB capacity-aware routing.** The existing least-connections algorithm is sufficient given the dispatch semaphore. Capacity-aware routing adds complexity without meaningful benefit when workers are configured with `max_concurrent=1`.
- **Dynamic `Retry-After` computation.** Not needed — the facade never sees 429 under this design.
- **Per-document priority queuing.** The semaphore is FIFO by default (Tokio's `Semaphore` implementation). Priority scheduling is a future concern.
