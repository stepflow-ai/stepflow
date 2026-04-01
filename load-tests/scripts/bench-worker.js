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
const mediumLoadLatency = new Trend('medium_load_latency');  // ≤50 VUs
const highLoadLatency = new Trend('high_load_latency');      // >50 VUs

// Stepflow service configuration
const STEPFLOW_URL = __ENV.STEPFLOW_URL || 'http://localhost:7837';
const API_BASE = `${STEPFLOW_URL}/api/v1`;
const RESULTS_DIR = __ENV.RESULTS_DIR || '';

// Python echo workflow
const workflowDefinition = JSON.parse(open('../workflows/bench-python-echo.json'));

// Test configuration - tuned for worker concurrency limits (lower than orchestrator)
const QUICK = __ENV.QUICK === 'true';

const fullStages = [
  { duration: '10s', target: 5 },      // Warm up
  { duration: '20s', target: 5 },      // Baseline
  { duration: '15s', target: 10 },     // Light load
  { duration: '30s', target: 10 },     // Sustain
  { duration: '15s', target: 25 },     // Medium load
  { duration: '30s', target: 25 },     // Sustain
  { duration: '15s', target: 50 },     // High load
  { duration: '30s', target: 50 },     // Sustain
  { duration: '15s', target: 100 },    // Push limits
  { duration: '30s', target: 100 },    // Sustain
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
  thresholds: {},
};

export function setup() {
  // Health check
  const healthCheck = http.get(`${STEPFLOW_URL}/api/v1/health`);
  if (healthCheck.status !== 200) {
    throw new Error(`Stepflow service not available at ${STEPFLOW_URL}`);
  }

  // Store workflow
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
  } else if (__VU <= 50) {
    mediumLoadLatency.add(duration);
  } else {
    highLoadLatency.add(duration);
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

  if (!success && __ITER % 50 === 0) {
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
  if (v['count'] === 0 || (v['avg'] === 0 && v['min'] === 0 && v['max'] === 0)) return null;
  return `  ${label.padEnd(22)} ${fmt(v['avg']).padStart(8)}  ${fmt(v['med']).padStart(8)}  ${fmt(v['p(90)']).padStart(8)}  ${fmt(v['p(95)']).padStart(8)}  ${fmt(v['max']).padStart(8)}`;
}

export function handleSummary(data) {
  const m = data.metrics;
  const completed = m.completed_runs ? m.completed_runs.values.count : 0;
  const failed = m.failed_runs ? m.failed_runs.values.count : 0;
  const total = completed + failed;
  const testDurationSecs = (data.state ? data.state.testRunDurationMs : 0) / 1000 || 1;
  const rps = (total / testDurationSecs).toFixed(1);
  const errorRate = total > 0 ? ((failed / total) * 100).toFixed(1) : '0.0';

  const header = `  ${'Load Level'.padEnd(22)} ${'avg'.padStart(8)}  ${'med'.padStart(8)}  ${'p90'.padStart(8)}  ${'p95'.padStart(8)}  ${'max'.padStart(8)}`;
  const sep = '  ' + '-'.repeat(header.length - 2);

  const rows = [
    trendRow('Overall', m.run_latency),
    trendRow('Low (≤10 VUs)', m.low_load_latency),
    trendRow('Medium (≤50 VUs)', m.medium_load_latency),
    trendRow('High (>50 VUs)', m.high_load_latency),
  ].filter(Boolean);

  const table = [
    '',
    `== bench-python-echo ==`,
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

  if (RESULTS_DIR) {
    const jsonSummary = {
      workflow: 'bench-python-echo',
      total_runs: total,
      completed,
      failed,
      error_rate_pct: parseFloat(errorRate),
      throughput_rps: parseFloat(rps),
      latency: {},
    };
    for (const [key, label] of [['run_latency', 'overall'], ['low_load_latency', 'low'], ['medium_load_latency', 'medium'], ['high_load_latency', 'high']]) {
      if (m[key] && m[key].values) {
        const v = m[key].values;
        jsonSummary.latency[label] = { avg: v['avg'], med: v['med'], p90: v['p(90)'], p95: v['p(95)'], min: v['min'], max: v['max'] };
      }
    }
    outputs[`${RESULTS_DIR}/bench-python-echo.json`] = JSON.stringify(jsonSummary, null, 2);
  }

  return outputs;
}
