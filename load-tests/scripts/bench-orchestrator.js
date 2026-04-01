import http from 'k6/http';
import { check } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';
// Custom metrics
const failureRate = new Rate('failed_requests');
const runLatency = new Trend('run_latency');
const completedRuns = new Counter('completed_runs');
const failedRuns = new Counter('failed_runs');

// Load-level metrics
const lowLoadLatency = new Trend('low_load_latency');       // ≤10 VUs
const mediumLoadLatency = new Trend('medium_load_latency');  // ≤100 VUs
const highLoadLatency = new Trend('high_load_latency');      // ≤500 VUs
const extremeLoadLatency = new Trend('extreme_load_latency'); // >500 VUs

// Get workflow type from environment variable
const WORKFLOW_TYPE = __ENV.WORKFLOW_TYPE || 'bench-noop';
const RESULTS_DIR = __ENV.RESULTS_DIR || '';

// Valid workflow types and their files
const workflowFiles = {
  'bench-noop': '../workflows/bench-noop.json',
  'bench-noop-1': '../workflows/bench-noop-1.json',
  'bench-noop-parallel': '../workflows/bench-noop-parallel.json',
  'bench-noop-100': '../workflows/bench-noop-100.json',
  'bench-sleep-10ms': '../workflows/bench-sleep-10ms.json',
  'bench-sleep-100ms': '../workflows/bench-sleep-100ms.json',
  'bench-routed-noop': '../workflows/bench-routed-noop.json',
  'bench-rust-echo': '../workflows/bench-rust-echo.json',
  'bench-python-echo': '../workflows/bench-python-echo.json',
};

if (!workflowFiles[WORKFLOW_TYPE]) {
  throw new Error(
    `Unknown WORKFLOW_TYPE: ${WORKFLOW_TYPE}. Valid types: ${Object.keys(workflowFiles).join(', ')}`
  );
}

const workflowDefinition = JSON.parse(open(workflowFiles[WORKFLOW_TYPE]));

// Stepflow service configuration
const STEPFLOW_URL = __ENV.STEPFLOW_URL || 'http://localhost:7837';
const API_BASE = `${STEPFLOW_URL}/api/v1`;

// Test configuration - aggressive ramp to find throughput ceiling
const QUICK = __ENV.QUICK === 'true';

const fullStages = [
  { duration: '10s', target: 10 },     // Warm up
  { duration: '20s', target: 10 },     // Baseline at low load
  { duration: '15s', target: 100 },    // Ramp to medium
  { duration: '30s', target: 100 },    // Sustain medium
  { duration: '15s', target: 500 },    // Ramp to high
  { duration: '30s', target: 500 },    // Sustain high
  { duration: '15s', target: 1000 },   // Ramp to extreme
  { duration: '30s', target: 1000 },   // Sustain extreme
  { duration: '15s', target: 0 },      // Ramp down
];

const quickStages = [
  { duration: '5s', target: 10 },      // Warm up
  { duration: '10s', target: 10 },     // Low load
  { duration: '5s', target: 100 },     // Ramp
  { duration: '10s', target: 100 },    // Medium load
  { duration: '5s', target: 500 },     // Ramp
  { duration: '10s', target: 500 },    // High load
  { duration: '5s', target: 0 },       // Ramp down
];

export const options = {
  stages: QUICK ? quickStages : fullStages,
  // No thresholds — benchmarks measure limits, not enforce them.
  // Threshold failures would just add noise to the output.
  thresholds: {},
};

export function setup() {
  // Health check
  const healthCheck = http.get(`${STEPFLOW_URL}/api/v1/health`);
  if (healthCheck.status !== 200) {
    throw new Error(`Stepflow service not available at ${STEPFLOW_URL}`);
  }

  // Store workflow definition
  const storeParams = {
    headers: { 'Content-Type': 'application/json' },
    timeout: '30s',
  };

  const flowPayload = { flow: workflowDefinition };
  const storeResponse = http.post(
    `${API_BASE}/flows`,
    JSON.stringify(flowPayload),
    storeParams
  );

  if (storeResponse.status !== 200) {
    throw new Error(
      `Failed to store workflow: ${storeResponse.status} - ${storeResponse.body}`
    );
  }

  const storeResult = JSON.parse(storeResponse.body);
  if (!storeResult.flowId) {
    throw new Error(`No flowId returned: ${storeResponse.body}`);
  }

  return { flowId: storeResult.flowId };
}

export default function (data) {
  const runPayload = {
    flowId: data.flowId,
    input: [{ data: 'bench' }],
    wait: true,
  };

  const params = {
    headers: { 'Content-Type': 'application/json' },
    timeout: '120s',
  };

  const startTime = Date.now();
  const response = http.post(
    `${API_BASE}/runs`,
    JSON.stringify(runPayload),
    params
  );
  const duration = Date.now() - startTime;

  // Record latency
  runLatency.add(duration);

  // Bucket by load level
  if (__VU <= 10) {
    lowLoadLatency.add(duration);
  } else if (__VU <= 100) {
    mediumLoadLatency.add(duration);
  } else if (__VU <= 500) {
    highLoadLatency.add(duration);
  } else {
    extremeLoadLatency.add(duration);
  }

  const success = check(response, {
    'status is 200': (r) => r.status === 200,
    'response has summary': (r) => {
      try {
        return JSON.parse(r.body).summary !== undefined;
      } catch (e) {
        return false;
      }
    },
  });

  if (success) {
    completedRuns.add(1);
  } else {
    failedRuns.add(1);
  }
  failureRate.add(!success);

  if (!success && __ITER % 100 === 0) {
    console.error(`Request failed: ${response.status} - ${response.body}`);
  }
}

// ---------------------------------------------------------------------------
// Custom summary — print a concise results table
// ---------------------------------------------------------------------------

function fmt(ms) {
  if (ms === undefined || ms === null) return '-';
  if (ms < 1) return '<1ms';
  if (ms < 1000) return `${Math.round(ms)}ms`;
  return `${(ms / 1000).toFixed(2)}s`;
}

function trendRow(label, trend) {
  if (!trend || !trend.values) return null;
  const v = trend.values;
  // Skip rows with no actual data (metric created but no .add() calls)
  if (v['count'] === 0 || (v['avg'] === 0 && v['min'] === 0 && v['max'] === 0)) return null;
  return `  ${label.padEnd(22)} ${fmt(v['avg']).padStart(8)}  ${fmt(v['med']).padStart(8)}  ${fmt(v['p(90)']).padStart(8)}  ${fmt(v['p(95)']).padStart(8)}  ${fmt(v['max']).padStart(8)}`;
}

export function handleSummary(data) {
  const m = data.metrics;
  const completed = m.completed_runs ? m.completed_runs.values.count : 0;
  const failed = m.failed_runs ? m.failed_runs.values.count : 0;
  const total = completed + failed;
  const duration = m.iteration_duration ? m.iteration_duration.values['max'] : 0;
  const testDurationSecs = (data.state ? data.state.testRunDurationMs : 0) / 1000 || 1;
  const rps = (total / testDurationSecs).toFixed(1);
  const errorRate = total > 0 ? ((failed / total) * 100).toFixed(1) : '0.0';

  const header = `  ${'Load Level'.padEnd(22)} ${'avg'.padStart(8)}  ${'med'.padStart(8)}  ${'p90'.padStart(8)}  ${'p95'.padStart(8)}  ${'max'.padStart(8)}`;
  const sep = '  ' + '-'.repeat(header.length - 2);

  const rows = [
    trendRow('Overall', m.run_latency),
    trendRow('Low (≤10 VUs)', m.low_load_latency),
    trendRow('Medium (≤100 VUs)', m.medium_load_latency),
    trendRow('High (≤500 VUs)', m.high_load_latency),
    trendRow('Extreme (>500 VUs)', m.extreme_load_latency),
  ].filter(Boolean);

  const table = [
    '',
    `== ${WORKFLOW_TYPE} ==`,
    '',
    `  Total runs:    ${total}`,
    `  Completed:     ${completed}`,
    `  Failed:        ${failed} (${errorRate}%)`,
    `  Throughput:    ${rps} runs/sec`,
    '',
    header,
    sep,
    ...rows,
    sep,
    '',
  ].join('\n');

  const outputs = { stdout: table + '\n' };

  // Write JSON summary if RESULTS_DIR is set
  if (RESULTS_DIR) {
    const jsonSummary = {
      workflow: WORKFLOW_TYPE,
      total_runs: total,
      completed,
      failed,
      error_rate_pct: parseFloat(errorRate),
      throughput_rps: parseFloat(rps),
      latency: {},
    };
    for (const [key, label] of [['run_latency', 'overall'], ['low_load_latency', 'low'], ['medium_load_latency', 'medium'], ['high_load_latency', 'high'], ['extreme_load_latency', 'extreme']]) {
      if (m[key] && m[key].values) {
        const v = m[key].values;
        jsonSummary.latency[label] = { avg: v['avg'], med: v['med'], p90: v['p(90)'], p95: v['p(95)'], min: v['min'], max: v['max'] };
      }
    }
    outputs[`${RESULTS_DIR}/${WORKFLOW_TYPE}.json`] = JSON.stringify(jsonSummary, null, 2);
  }

  return outputs;
}
