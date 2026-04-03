#!/bin/bash
# Copyright 2025 DataStax Inc.
#
# Licensed under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License. You may obtain a copy of
# the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
# License for the specific language governing permissions and limitations under
# the License.

# ============================================================================
# Stepflow Benchmark Suite
# ============================================================================
#
# Single command to build, start the server, run benchmarks, and print results.
#
# Usage:
#   ./bench.sh                    # Run all orchestrator benchmarks
#   ./bench.sh --quick            # Shorter test (fewer VUs, shorter stages)
#   ./bench.sh --skip-python      # Skip Python worker benchmarks
#   ./bench.sh --skip-routed      # Skip routed-noop (needs bench worker)
#   ./bench.sh --url URL          # Use an already-running server
#
# Prerequisites:
#   - Rust toolchain (cargo)
#   - k6 (https://k6.io/docs/getting-started/installation/)
#   - For Python benchmarks: Python + uv
#
# ============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PORT=7837
STEPFLOW_URL=""
SKIP_PYTHON=true   # Skip Python by default (requires extra setup)
SKIP_ROUTED=true   # Skip routed by default (requires bench worker)
SKIP_RUST=true     # Skip Rust echo by default (requires bench worker echo mode)
EXTERNAL_SERVER=false
QUICK=false
WORKER_CONCURRENCY=100  # Max concurrent tasks per worker process
SERVER_PID=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --url)
            STEPFLOW_URL="$2"
            EXTERNAL_SERVER=true
            shift 2
            ;;
        --port)
            PORT="$2"
            shift 2
            ;;
        --skip-python)
            SKIP_PYTHON=true
            shift
            ;;
        --with-python)
            SKIP_PYTHON=false
            shift
            ;;
        --skip-routed)
            SKIP_ROUTED=true
            shift
            ;;
        --with-routed)
            SKIP_ROUTED=false
            shift
            ;;
        --skip-rust)
            SKIP_RUST=true
            shift
            ;;
        --with-rust)
            SKIP_RUST=false
            shift
            ;;
        --concurrency)
            WORKER_CONCURRENCY="$2"
            shift 2
            ;;
        --quick)
            QUICK=true
            shift
            ;;
        -h|--help)
            echo "Stepflow Benchmark Suite"
            echo ""
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --url URL          Use an already-running server (skip build/start)"
            echo "  --port PORT        Server port (default: 7837, ignored with --url)"
            echo "  --quick            Shorter test run (good for smoke testing)"
            echo "  --concurrency N    Max concurrent tasks per worker (default: 100)"
            echo "  --with-python      Include Python worker benchmarks (requires uv)"
            echo "  --with-rust        Include Rust echo worker benchmark (requires bench worker)"
            echo "  --with-routed      Include routed-noop (requires bench worker running)"
            echo "  --skip-python      Skip Python worker benchmarks (default)"
            echo "  --skip-rust        Skip Rust echo worker benchmarks (default)"
            echo "  --skip-routed      Skip routed-noop benchmarks (default)"
            echo "  -h, --help         Show this help"
            echo ""
            echo "Benchmarks run by default (builtin-only, no external deps):"
            echo "  bench-noop-1         Single noop step (per-flow overhead baseline)"
            echo "  bench-noop-parallel  10 parallel noop steps (per-step scaling)"
            echo "  bench-noop-100       100 parallel noop steps (per-step scaling)"
            echo "  bench-sleep-10ms     Concurrent flow capacity (10ms simulated latency)"
            echo "  bench-sleep-100ms    Concurrent flow capacity (100ms simulated latency)"
            echo ""
            echo "Optional benchmarks (require extra setup):"
            echo "  bench-rust-echo      Rust worker overhead (needs: stepflow-bench-worker echo)"
            echo "  bench-routed-noop    gRPC dispatch path (needs: stepflow-bench-worker instant)"
            echo "  bench-python-echo    Python worker overhead (needs: Python + uv)"
            exit 0
            ;;
        *)
            echo "Unknown option: $1 (try --help)"
            exit 1
            ;;
    esac
done

if [ "$EXTERNAL_SERVER" = false ]; then
    # Use 127.0.0.1 (not localhost) to match the orchestrator's advertised
    # address in TaskContext, so workers reuse the same gRPC channel.
    STEPFLOW_URL="http://127.0.0.1:${PORT}"
fi

# Results directory
RESULTS_DIR="$SCRIPT_DIR/results/bench_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$RESULTS_DIR"

# --- Checks ---

if ! command -v k6 &> /dev/null; then
    echo "Error: k6 is not installed"
    echo "Install: https://grafana.com/docs/k6/latest/set-up/install-k6/"
    exit 1
fi

# --- Cleanup handler ---

RUST_ECHO_PID=""
INSTANT_PID=""

cleanup() {
    for pid_var in RUST_ECHO_PID INSTANT_PID SERVER_PID; do
        pid="${!pid_var}"
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
            wait "$pid" 2>/dev/null || true
        fi
    done
}
trap cleanup EXIT

# --- Build and start server ---

if [ "$EXTERNAL_SERVER" = false ]; then
    echo "Building Stepflow (release)..."
    cd "$SCRIPT_DIR/../stepflow-rs"
    cargo build --release -p stepflow-server 2>&1 | tail -1

    CONFIG_PATH="$SCRIPT_DIR/workflows/bench-config.yml"
    if [ ! -f "$CONFIG_PATH" ]; then
        echo "Error: Config not found at $CONFIG_PATH"
        exit 1
    fi

    # Check port
    if lsof -Pi :"$PORT" -sTCP:LISTEN -t >/dev/null 2>&1; then
        echo "Error: Port $PORT already in use (lsof -i :$PORT to check)"
        exit 1
    fi

    # Export so Python worker subprocess inherits the concurrency setting
    export STEPFLOW_MAX_CONCURRENT="$WORKER_CONCURRENCY"

    echo "Starting Stepflow server on port $PORT (worker concurrency: $WORKER_CONCURRENCY)..."
    ./target/release/stepflow-server \
        --port="$PORT" \
        --config="$CONFIG_PATH" \
        --log-level=warn \
        > "$RESULTS_DIR/stepflow-server.log" 2>&1 &
    SERVER_PID=$!

    # Wait for server to be ready
    echo -n "Waiting for server"
    for i in $(seq 1 30); do
        if curl -sf "$STEPFLOW_URL/api/v1/health" > /dev/null 2>&1; then
            echo " ready."
            break
        fi
        if ! kill -0 "$SERVER_PID" 2>/dev/null; then
            echo ""
            echo "Error: Server process died. Check $RESULTS_DIR/stepflow-server.log"
            exit 1
        fi
        echo -n "."
        sleep 1
    done

    if ! curl -sf "$STEPFLOW_URL/api/v1/health" > /dev/null 2>&1; then
        echo ""
        echo "Error: Server failed to start within 30s"
        exit 1
    fi
else
    echo "Using external server at $STEPFLOW_URL"
    if ! curl -sf "$STEPFLOW_URL/api/v1/health" > /dev/null 2>&1; then
        echo "Error: Server not responding at $STEPFLOW_URL"
        exit 1
    fi
    echo "Server is healthy."
fi

# --- Start bench workers if needed ---

BENCH_WORKER_BIN="$SCRIPT_DIR/../sdks/rust/target/release/stepflow-bench-worker"

if [ "$SKIP_RUST" = false ] || [ "$SKIP_ROUTED" = false ]; then
    # Build the bench worker binary
    echo "Building stepflow-bench-worker (release)..."
    cd "$SCRIPT_DIR/../sdks/rust"
    cargo build --release -p stepflow-bench-worker 2>&1 | tail -1
fi

if [ "$SKIP_RUST" = false ]; then
    echo "Starting Rust echo worker (concurrency: $WORKER_CONCURRENCY)..."
    "$BENCH_WORKER_BIN" echo \
        --tasks-url "$STEPFLOW_URL" \
        --queue-name rust \
        --max-concurrent "$WORKER_CONCURRENCY" \
        > "$RESULTS_DIR/rust-echo-worker.log" 2>&1 &
    RUST_ECHO_PID=$!
    sleep 2
    if ! kill -0 "$RUST_ECHO_PID" 2>/dev/null; then
        echo "Warning: Rust echo worker failed to start. Check $RESULTS_DIR/rust-echo-worker.log"
        SKIP_RUST=true
    else
        echo "Rust echo worker started (pid $RUST_ECHO_PID)"
    fi
fi

if [ "$SKIP_ROUTED" = false ]; then
    echo "Starting instant-completion worker..."
    "$BENCH_WORKER_BIN" instant \
        --tasks-url "$STEPFLOW_URL" \
        --queue-name bench \
        --workers 10 \
        > "$RESULTS_DIR/instant-worker.log" 2>&1 &
    INSTANT_PID=$!
    sleep 2
    if ! kill -0 "$INSTANT_PID" 2>/dev/null; then
        echo "Warning: Instant worker failed to start. Check $RESULTS_DIR/instant-worker.log"
        SKIP_ROUTED=true
    else
        echo "Instant worker started (pid $INSTANT_PID)"
    fi
fi

# --- Run benchmarks ---

cd "$SCRIPT_DIR/scripts"

# Override k6 stages for quick mode
K6_QUICK_OPTS=""
if [ "$QUICK" = true ]; then
    # k6 doesn't support overriding stages via CLI, so we use a shorter duration multiplier
    # by setting an env var that the script can check
    K6_QUICK_OPTS="--env QUICK=true"
    echo ""
    echo "Quick mode: running with reduced load (good for smoke testing)"
fi

# Run worker-dependent benchmarks FIRST (lower load, avoids port exhaustion
# from the high-throughput orchestrator benchmarks that follow).

echo ""
echo "============================================"
echo "  Stepflow Benchmark Suite"
echo "============================================"
echo ""

# Rust echo worker (requires bench worker echo mode)
if [ "$SKIP_RUST" = false ]; then
    echo "--- Running: bench-rust-echo ---"
    k6 run --quiet \
        --env STEPFLOW_URL="$STEPFLOW_URL" \
        --env WORKFLOW_TYPE="bench-rust-echo" \
        --env RESULTS_DIR="$RESULTS_DIR" \
        $K6_QUICK_OPTS \
        bench-orchestrator.js || true
    sleep 2
fi

# Routed noop (requires bench worker instant mode)
if [ "$SKIP_ROUTED" = false ]; then
    echo "--- Running: bench-routed-noop ---"
    k6 run --quiet \
        --env STEPFLOW_URL="$STEPFLOW_URL" \
        --env WORKFLOW_TYPE="bench-routed-noop" \
        --env RESULTS_DIR="$RESULTS_DIR" \
        $K6_QUICK_OPTS \
        bench-orchestrator.js || true
    sleep 2
fi

# Python worker (requires Python + uv)
if [ "$SKIP_PYTHON" = false ]; then
    echo "--- Running: bench-python-echo ---"
    k6 run --quiet \
        --env STEPFLOW_URL="$STEPFLOW_URL" \
        --env RESULTS_DIR="$RESULTS_DIR" \
        $K6_QUICK_OPTS \
        bench-worker.js || true
    sleep 2
fi

# Orchestrator benchmarks (builtin-only, high throughput — run last to
# avoid ephemeral port exhaustion affecting worker benchmarks above).
ORCHESTRATOR_WORKFLOWS=("bench-noop-1" "bench-noop-parallel" "bench-noop-100" "bench-sleep-10ms" "bench-sleep-100ms")

for workflow in "${ORCHESTRATOR_WORKFLOWS[@]}"; do
    echo "--- Running: $workflow ---"
    k6 run --quiet \
        --env STEPFLOW_URL="$STEPFLOW_URL" \
        --env WORKFLOW_TYPE="$workflow" \
        --env RESULTS_DIR="$RESULTS_DIR" \
        $K6_QUICK_OPTS \
        bench-orchestrator.js || true
    sleep 2
done

# --- Final summary ---

echo ""
echo "============================================"
echo "  Results saved to: $RESULTS_DIR/"
echo "============================================"
echo ""

# Print combined JSON results if any exist
JSON_FILES=("$RESULTS_DIR"/*.json)
if [ ${#JSON_FILES[@]} -gt 0 ] && [ -f "${JSON_FILES[0]}" ]; then
    echo "Summary (throughput across all benchmarks):"
    echo ""
    printf "  %-24s %12s %10s %10s %10s %10s\n" "Workflow" "Runs/sec" "avg" "med" "p95" "Errors"
    printf "  %-24s %12s %10s %10s %10s %10s\n" "--------" "--------" "---" "---" "---" "------"
    for f in "$RESULTS_DIR"/*.json; do
        [ -f "$f" ] || continue
        if command -v python3 &> /dev/null; then
            python3 -c "
import json, sys
d = json.load(open('$f'))
name = d.get('workflow', '?')
rps = d.get('throughput_rps', 0)
err = d.get('error_rate_pct', 0)
lat = d.get('latency', {}).get('overall', {})
avg = lat.get('avg', 0)
med = lat.get('med', 0)
p95 = lat.get('p95', 0)
def fmt(ms):
    if ms < 1: return '<1ms'
    if ms < 1000: return f'{ms:.0f}ms'
    return f'{ms/1000:.2f}s'
print(f'  {name:<24} {rps:>12.1f} {fmt(avg):>10} {fmt(med):>10} {fmt(p95):>10} {err:>9.1f}%')
" 2>/dev/null || true
        fi
    done
    echo ""

    # --- Derived analysis ---
    if command -v python3 &> /dev/null; then
        python3 -c "
import json, glob, os

# Load all results
results = {}
for f in sorted(glob.glob(os.path.join('$RESULTS_DIR', '*.json'))):
    d = json.load(open(f))
    results[d['workflow']] = d

# --- Per-step analysis (from noop scaling series) ---
noop_series = []
for name, steps in [('bench-noop-1', 1), ('bench-noop-parallel', 10), ('bench-noop-100', 100)]:
    if name in results:
        r = results[name]
        med = r.get('latency', {}).get('overall', {}).get('med', 0)
        rps = r.get('throughput_rps', 0)
        noop_series.append((name, steps, med, rps))

if len(noop_series) >= 2:
    print('Derived Metrics:')
    print('')

    # Steps/sec throughput
    print('  Steps/sec throughput:')
    for name, steps, med, rps in noop_series:
        sps = rps * steps
        print(f'    {name:<24} {sps:>10,.0f} steps/sec  ({steps} steps x {rps:,.0f} runs/sec)')
    print('')

    # Per-step overhead estimation
    # Model: median_ms = flow_overhead + steps * per_step_overhead
    # Solve using least-squares with 2+ points
    if len(noop_series) >= 2:
        # Simple linear regression: t = a + b*n
        n_vals = [s[1] for s in noop_series]
        t_vals = [s[2] for s in noop_series]
        n_mean = sum(n_vals) / len(n_vals)
        t_mean = sum(t_vals) / len(t_vals)
        num = sum((n - n_mean) * (t - t_mean) for n, t in zip(n_vals, t_vals))
        den = sum((n - n_mean) ** 2 for n in n_vals)
        if den > 0:
            per_step = num / den  # ms per step
            flow_overhead = t_mean - per_step * n_mean
            print(f'  Per-flow overhead:      ~{max(0, flow_overhead):.1f}ms  (estimated from noop scaling)')
            print(f'  Per-step overhead:      ~{per_step:.2f}ms  (median, parallel scheduling)')
            print('')

# --- Concurrency estimation (from sleep benchmarks, Little's Law) ---
sleep_data = []
for name, sleep_ms in [('bench-sleep-10ms', 10), ('bench-sleep-100ms', 100)]:
    if name in results:
        r = results[name]
        med = r.get('latency', {}).get('overall', {}).get('med', 0)
        rps = r.get('throughput_rps', 0)
        if rps > 0 and med > 0:
            concurrency = rps * (med / 1000.0)  # Little's Law: L = lambda * W
            overhead = med - sleep_ms
            sleep_data.append((name, sleep_ms, rps, med, concurrency, overhead))

if sleep_data:
    print('  Concurrent flow capacity (Little\\'s Law: concurrency = throughput x latency):')
    for name, sleep_ms, rps, med, conc, overhead in sleep_data:
        print(f'    {name:<24} ~{conc:,.0f} concurrent flows  ({rps:,.0f} rps x {med:.0f}ms med, {overhead:.0f}ms overhead)')
    print('')

# --- Orchestrator capacity ---
if len(noop_series) >= 2:
    max_sps = max(rps * steps for _, steps, _, rps in noop_series)
    max_fps = max(rps for _, steps, _, rps in noop_series if steps == 1) if any(s == 1 for _, s, _, _ in noop_series) else noop_series[0][3]
    print(f'  Orchestrator ceiling (free components):')
    print(f'    Max steps/sec:       ~{max_sps:,.0f}')
    print(f'    Max flows/sec:       ~{max_fps:,.0f}  (single-step flows)')
    print('')

# --- Worker overhead and crossover analysis ---
# Collect per-task overhead from echo benchmarks
workers = []  # (label, overhead_ms, max_concurrent)

# Builtin: overhead from noop-1 median
if 'bench-noop-1' in results:
    noop1 = results['bench-noop-1']
    noop1_med = noop1.get('latency', {}).get('overall', {}).get('med', 0)
    if noop1_med > 0:
        workers.append(('Builtin', noop1_med, None, noop1.get('throughput_rps', 0)))

# Rust echo worker
if 'bench-rust-echo' in results:
    r = results['bench-rust-echo']
    med = r.get('latency', {}).get('overall', {}).get('med', 0)
    rps = r.get('throughput_rps', 0)
    if med > 0:
        workers.append(('Rust worker', med, 10, rps))

# Routed instant (bench worker)
if 'bench-routed-noop' in results:
    r = results['bench-routed-noop']
    med = r.get('latency', {}).get('overall', {}).get('med', 0)
    rps = r.get('throughput_rps', 0)
    if med > 0:
        workers.append(('Instant (gRPC)', med, None, rps))

# Python echo worker
if 'bench-python-echo' in results:
    r = results['bench-python-echo']
    med = r.get('latency', {}).get('overall', {}).get('med', 0)
    rps = r.get('throughput_rps', 0)
    if med > 0:
        workers.append(('Python worker', med, 4, rps))

if len(workers) >= 2:
    print('  Worker overhead (per-task gRPC + execution cost):')
    print(f'    {\"Backend\":<20} {\"Overhead\":>10} {\"Observed rps\":>14} {\"Plumbing bottleneck if task <\":>30}')
    print(f'    {\"-\" * 20} {\"-\" * 10} {\"-\" * 14} {\"-\" * 30}')
    for label, overhead, conc, rps in workers:
        print(f'    {label:<20} {overhead:>9.1f}ms {rps:>13,.0f} {overhead:>25.0f}ms')
    print('')

    # Throughput projection table
    print('  Projected throughput by component execution time:')
    compute_times = [0, 1, 10, 100, 1000]
    header = f'    {\"Compute time\":<14}'
    for label, _, _, _ in workers:
        header += f' {label:>16}'
    print(header)
    print(f'    {\"-\" * 14}' + ''.join(f' {\"-\" * 16}' for _ in workers))

    for ct in compute_times:
        row = f'    {str(ct) + \"ms\":<14}'
        for label, overhead, conc, observed_rps in workers:
            if ct == 0:
                rps_est = observed_rps
            else:
                total_ms = ct + overhead
                if conc is not None:
                    rps_est = conc / (total_ms / 1000.0)
                else:
                    # For builtin/instant, use observed throughput scaled
                    rps_est = 1000.0 / total_ms * (observed_rps * overhead / 1000.0) if overhead > 0 else observed_rps
                    # Simpler: just use Little's Law with observed concurrency
                    observed_conc = observed_rps * (overhead / 1000.0)
                    rps_est = observed_conc / (total_ms / 1000.0)
            row += f' {rps_est:>14,.0f}/s'
        print(row)
    print('')
" 2>/dev/null || true
    fi
fi

echo "To share results, copy the JSON files from $RESULTS_DIR/"
