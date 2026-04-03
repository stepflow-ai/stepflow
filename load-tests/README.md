# Stepflow Load Testing & Benchmarks

## Benchmarks (Orchestrator & Worker Performance)

Measure orchestrator throughput, concurrent flow capacity, and worker overhead.

### Quick Start

```bash
cd load-tests
./bench.sh              # Build, start server, run all benchmarks, print results
./bench.sh --quick      # Shorter run (good for smoke testing)
```

### What it measures

| Benchmark | What it tests |
|-----------|--------------|
| `bench-noop` | Pure orchestrator overhead — single noop step, instant return |
| `bench-noop-parallel` | Parallel step scheduling — 10 concurrent noop steps per run |
| `bench-sleep-10ms` | Concurrent flow capacity with 10ms simulated latency |
| `bench-sleep-100ms` | Concurrent flow capacity with 100ms simulated latency |
| `bench-routed-noop` | Full gRPC dispatch path (requires bench worker) |
| `bench-python-echo` | Python worker roundtrip overhead (requires Python + uv) |

### Output

Each benchmark prints a table like:

```
== bench-noop ==

  Total runs:    15234
  Completed:     15234
  Failed:        0 (0.0%)
  Throughput:    84.6 runs/sec

  Load Level                p50       p95       p99       min       max    count
  ----------------------------------------------------------------------------------
  Overall                   8ms      45ms     120ms       2ms     350ms    15234
  Low (≤10 VUs)             3ms       8ms      15ms       2ms      25ms     1200
  Medium (≤100 VUs)        12ms      35ms      55ms       3ms     120ms     5400
  High (≤500 VUs)          25ms      80ms     150ms       5ms     350ms     8634
```

A final summary table compares all benchmarks:

```
  Workflow                     Runs/sec        p50        p95    Errors
  --------                     --------        ---        ---    ------
  bench-noop                      84.6        8ms       45ms      0.0%
  bench-noop-parallel             42.1       18ms       90ms      0.0%
  bench-sleep-10ms                78.2       15ms       55ms      0.0%
  bench-sleep-100ms               55.3      110ms      180ms      0.0%
```

### Options

```bash
./bench.sh --help           # Show all options
./bench.sh --url URL        # Use an already-running server (skip build/start)
./bench.sh --with-python    # Include Python worker benchmarks
./bench.sh --with-routed    # Include routed-noop (needs bench worker running)
./bench.sh --port 8080      # Use a different port
```

### Running with the bench worker (for gRPC dispatch benchmarks)

```bash
# Terminal 1: Start the benchmark suite (will start the server)
./bench.sh --with-routed

# Terminal 2: Start the bench worker (before bench.sh reaches bench-routed-noop)
cd sdks/rust
cargo run -p stepflow-bench-worker --release -- --queue-name bench --workers 10
```

### Running individual benchmarks

```bash
# Start server manually
cd stepflow-rs
cargo run --release -- serve --port=7837 --config=../load-tests/workflows/bench-config.yml

# Run a specific benchmark
cd load-tests/scripts
k6 run --env WORKFLOW_TYPE=bench-noop bench-orchestrator.js
k6 run --env WORKFLOW_TYPE=bench-sleep-100ms bench-orchestrator.js
k6 run bench-worker.js  # Python worker benchmark
```

### Results

JSON results are saved to `results/bench_TIMESTAMP/` for sharing and comparison across hardware.

---

## Load Tests (End-to-End with OpenAI)

Older load tests that measure end-to-end performance including OpenAI API calls.

### Standalone Tests (No OpenAI)

```bash
cd load-tests
./scripts/start-standalone-service.sh       # Terminal 1
./scripts/run-single-standalone-tests.sh    # Terminal 2
```

Tests three workflow types with simple message creation:
- `rust-builtin-messages` — Direct Rust built-in components
- `python-custom-messages` — Python custom component server
- `python-udf-messages` — Python UDF with blob storage

### OpenAI Tests

```bash
export OPENAI_API_KEY="your-key"
cd load-tests
./scripts/start-service.sh                  # Terminal 1
./scripts/run-single-openai-tests.sh        # Terminal 2
```

---

## Prerequisites

- **Rust toolchain** (cargo)
- **k6**: `brew install k6` (macOS) or see [k6 installation](https://k6.io/docs/getting-started/installation/)
- **For Python benchmarks**: Python + uv
- **For OpenAI tests**: `OPENAI_API_KEY` environment variable
