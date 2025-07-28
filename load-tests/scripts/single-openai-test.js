import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend, Counter } from 'k6/metrics';

// Custom metrics
const failureRate = new Rate('failed_requests');
const responseTime = new Trend('response_time');
const httpReqDuration = new Trend('http_req_duration_custom');

// Load-level specific metrics
const lowLoadLatency = new Trend('low_load_latency');      // Up to 10 users
const mediumLoadLatency = new Trend('medium_load_latency');  // Up to 100 users
const highLoadLatency = new Trend('high_load_latency');    // Up to 500 users

// Request counters for each load level (k6 will calculate RPS automatically)
const lowLoadRequests = new Counter('low_load_requests');       // Request count at low load (≤10 users)
const mediumLoadRequests = new Counter('medium_load_requests');   // Request count at medium load (≤100 users)
const highLoadRequests = new Counter('high_load_requests');     // Request count at high load (≤500 users)

// Load test prompts
const testPrompts = JSON.parse(open('./test-prompts.json'));

// Get workflow type from environment variable
const WORKFLOW_TYPE = __ENV.WORKFLOW_TYPE || 'rust-builtin-openai';

// Aggressive load test stages to find limits quickly
export const options = {
  stages: [
    // { duration: '10s', target: 50 },   // Medium load ramp
    // { duration: '30s', target: 100 },  // Medium load tier
    // { duration: '10s', target: 200 },  // High load ramp
    // { duration: '10s', target: 300 },  // High load tier
    { duration: '20s', target: 1000 },  // Peak load test
    { duration: '40s', target: 1000 },  // Brief peak sustain
    // { duration: '10s', target: 0 },    // Quick ramp down
  ],
  thresholds: {
    http_req_duration: ['p(95)<60000'], // 95% of requests must complete within 60s (more lenient for high load)
    failed_requests: ['rate<0.3'],       // Allow up to 30% failures for very aggressive testing

    // Load-specific latency thresholds
    low_load_latency: ['p(95)<15000', 'p(50)<8000'],     // Should be fast at low load (≤10 users)
    medium_load_latency: ['p(95)<30000', 'p(50)<15000'], // Moderate degradation (≤100 users)
    high_load_latency: ['p(95)<60000'],                   // Allow high latency under extreme stress (≤500 users)

    // Request count thresholds (informational - k6 will show RPS automatically)
    low_load_requests: ['count>0'],     // Should process some requests at low load
    medium_load_requests: ['count>0'],  // Should process requests at medium load
    high_load_requests: ['count>0'],    // May have fewer requests at high load due to failures
  },
};

// Stepflow service configuration
const STEPFLOW_URL = __ENV.STEPFLOW_URL || 'http://localhost:7837';
const API_BASE = `${STEPFLOW_URL}/api/v1`;

// Workflow definitions
const workflowDefinitions = {
  'python-udf-openai': JSON.parse(open('../workflows/python-udf-openai.json')),
  'python-custom-openai': JSON.parse(open('../workflows/python-custom-openai.json')),
  'rust-builtin-openai': JSON.parse(open('../workflows/rust-builtin-openai.json'))
};

// Workflow configurations
const workflows = {
  'python-udf-openai': {
    name: 'python-udf-openai',
    description: 'Python UDF OpenAI'
  },
  'python-custom-openai': {
    name: 'python-custom-openai',
    description: 'Python Custom Component OpenAI'
  },
  'rust-builtin-openai': {
    name: 'rust-builtin-openai',
    description: 'Rust Built-in OpenAI'
  }
};

function getRandomPrompt() {
  return testPrompts[Math.floor(Math.random() * testPrompts.length)];
}

export function setup() {
  console.log(`Starting single workflow load test for: ${WORKFLOW_TYPE}`);
  console.log(`Stepflow URL: ${STEPFLOW_URL}`);
  console.log(`Using ${testPrompts.length} test prompts`);

  if (!workflows[WORKFLOW_TYPE]) {
    throw new Error(`Unknown workflow type: ${WORKFLOW_TYPE}. Valid types: ${Object.keys(workflows).join(', ')}`);
  }

  const workflow = workflows[WORKFLOW_TYPE];

  // Check if service is available
  const healthCheck = http.get(`${STEPFLOW_URL}/api/v1/health`);
  if (healthCheck.status !== 200) {
    throw new Error(`Stepflow service is not available at ${STEPFLOW_URL}`);
  }

  console.log('Stepflow service is healthy...');

  // Store workflow definition and get hash
  console.log(`Storing workflow: ${workflow.description}...`);
  const storeFlowParams = {
    headers: {
      'Content-Type': 'application/json',
    },
    timeout: '30s',
  };

  const flowPayload = {
    flow: workflowDefinitions[workflow.name]
  };

  const storeResponse = http.post(`${API_BASE}/flows`, JSON.stringify(flowPayload), storeFlowParams);

  if (storeResponse.status !== 200) {
    throw new Error(`Failed to store workflow ${workflow.name}: ${storeResponse.status} - ${storeResponse.body}`);
  }

  const storeResult = JSON.parse(storeResponse.body);
  if (!storeResult.flowHash) {
    throw new Error(`No flowHash returned for workflow ${workflow.name}: ${storeResponse.body}`);
  }

  console.log(`✓ Stored ${workflow.description} with hash: ${storeResult.flowHash.substring(0, 8)}...`);
  console.log('Starting load test...');

  return {
    flowHash: storeResult.flowHash,
    workflow: workflow
  };
}

export default function (data) {
  const prompt = getRandomPrompt();

  const runPayload = {
    flowHash: data.flowHash,
    input: {
      prompt: prompt.prompt,
      system_message: prompt.system_message
    }
  };

  const params = {
    headers: {
      'Content-Type': 'application/json',
    },
    timeout: '60s',
  };

  const startTime = Date.now();

  // Execute workflow run
  const response = http.post(`${API_BASE}/runs`, JSON.stringify(runPayload), params);

  const endTime = Date.now();
  const duration = endTime - startTime;

  // Record metrics
  responseTime.add(duration);
  httpReqDuration.add(duration);

  // Record load-specific latency and request count based on current VU count
  const currentVUs = __VU; // Current virtual user count approximation
  if (currentVUs <= 10) {
    lowLoadLatency.add(duration);
    lowLoadRequests.add(1); // Count this request for low load
  } else if (currentVUs <= 100) {
    mediumLoadLatency.add(duration);
    mediumLoadRequests.add(1); // Count this request for medium load
  } else {
    highLoadLatency.add(duration);
    highLoadRequests.add(1); // Count this request for high load
  }

  // Check response
  const success = check(response, {
    'status is 200': (r) => r.status === 200,
    'response has runId': (r) => {
      try {
        const body = JSON.parse(r.body);
        return body.runId !== undefined;
      } catch (e) {
        return false;
      }
    },
    'response has result or is running': (r) => {
      try {
        const body = JSON.parse(r.body);
        return body.result !== undefined || body.status === 'running';
      } catch (e) {
        return false;
      }
    },
    'if completed, contains OpenAI output': (r) => {
      try {
        const body = JSON.parse(r.body);
        if (body.status === 'completed') {
          return body.result && body.result.result && body.result.result.response !== undefined;
        }
        return true;
      } catch (e) {
        return false;
      }
    }
  });

  failureRate.add(!success);

  if (!success) {
    console.error(`Request failed for ${data.workflow.description}`);
    console.error(`Status: ${response.status}`);
    console.error(`Response: ${response.body}`);
  } else {
    const result = JSON.parse(response.body);
    if (__ITER % 100 === 0) { // Log every 10th successful request to reduce noise
      console.log(`✓ ${data.workflow.description} completed in ${duration}ms (status: ${result.status})`);
    }
  }

  // Very brief pause between requests for aggressive testing
  sleep(0.05);
}

export function teardown(data) {
  console.log(`Load test completed for: ${data.workflow.description}`);
  console.log(`Flow hash used: ${data.flowHash.substring(0, 8)}...`);
  console.log('');
  console.log('Load Level Performance Summary:');
  console.log('  Low Load (≤10 users): Check low_load_requests (RPS) and low_load_latency metrics');
  console.log('  Medium Load (≤100 users): Check medium_load_requests (RPS) and medium_load_latency metrics');
  console.log('  High Load (≤500 users): Check high_load_requests (RPS) and high_load_latency metrics');
  console.log('');
  console.log('Note: Request counter metrics show both total count and rate (RPS) in k6 output.');
  console.log('Higher RPS with lower latency indicates better performance scaling.');
}