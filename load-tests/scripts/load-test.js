import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';

// Custom metrics - Overall
const failureRate = new Rate('failed_requests');
const responseTime = new Trend('response_time');
const storeFlowTime = new Trend('store_flow_time');
const executeRunTime = new Trend('execute_run_time');

// Custom metrics - Per workflow type
const pythonUdfResponseTime = new Trend('python_udf_response_time');
const pythonUdfFailureRate = new Rate('python_udf_failure_rate');

const pythonCustomResponseTime = new Trend('python_custom_response_time');
const pythonCustomFailureRate = new Rate('python_custom_failure_rate');

const rustBuiltinResponseTime = new Trend('rust_builtin_response_time');
const rustBuiltinFailureRate = new Rate('rust_builtin_failure_rate');

// Request counters per workflow type
const pythonUdfRequests = new Rate('python_udf_requests');
const pythonCustomRequests = new Rate('python_custom_requests');
const rustBuiltinRequests = new Rate('rust_builtin_requests');

// Load test prompts
const testPrompts = JSON.parse(open('./test-prompts.json'));

// Test configuration
export const options = {
  stages: [
    { duration: '30s', target: 1 },   // Warm up with 1 user
    { duration: '1m', target: 5 },    // Ramp up to 5 users
    { duration: '2m', target: 5 },    // Stay at 5 users
    { duration: '1m', target: 10 },   // Ramp up to 10 users
    { duration: '2m', target: 10 },   // Stay at 10 users
    { duration: '1m', target: 20 },   // Ramp up to 20 users
    { duration: '2m', target: 20 },   // Stay at 20 users
    { duration: '30s', target: 0 },   // Ramp down to 0 users
  ],
  thresholds: {
    // Overall thresholds
    http_req_duration: ['p(95)<30000'], // 95% of requests must complete within 30s
    failed_requests: ['rate<0.1'],       // Failure rate should be less than 10%
    
    // Per-workflow type thresholds for performance comparison
    python_udf_response_time: ['p(95)<35000', 'p(50)<15000'],        // Python UDF expected to be slower
    python_udf_failure_rate: ['rate<0.15'],                          // Allow slightly higher failure rate
    
    python_custom_response_time: ['p(95)<25000', 'p(50)<12000'],     // Custom component should be faster than UDF
    python_custom_failure_rate: ['rate<0.1'],                       // Standard failure rate
    
    rust_builtin_response_time: ['p(95)<20000', 'p(50)<8000'],      // Built-in should be fastest
    rust_builtin_failure_rate: ['rate<0.05'],                       // Should be most reliable
  },
};

// Stepflow service configuration
const STEPFLOW_URL = __ENV.STEPFLOW_URL || 'http://localhost:7837';
const API_BASE = `${STEPFLOW_URL}/api/v1`;

// Workflow definitions (loaded from YAML files)
const workflowDefinitions = {
  'python-udf-openai': JSON.parse(open('../workflows/python-udf-openai.json')),
  'python-custom-openai': JSON.parse(open('../workflows/python-custom-openai.json')),
  'rust-builtin-openai': JSON.parse(open('../workflows/rust-builtin-openai.json'))
};

// Workflow configurations
const workflows = [
  {
    name: 'python-udf-openai',
    description: 'Python UDF OpenAI'
  },
  {
    name: 'python-custom-openai',
    description: 'Python Custom Component OpenAI'
  },
  {
    name: 'rust-builtin-openai',
    description: 'Rust Built-in OpenAI'
  }
];

function getRandomPrompt() {
  return testPrompts[Math.floor(Math.random() * testPrompts.length)];
}

function getRandomWorkflow() {
  return workflows[Math.floor(Math.random() * workflows.length)];
}

// Global storage for flow hashes (shared across all VUs)
let flowHashes = {};

export function setup() {
  console.log('Starting Stepflow load test...');
  console.log(`Stepflow URL: ${STEPFLOW_URL}`);
  console.log(`Using ${testPrompts.length} test prompts`);
  console.log(`Testing ${workflows.length} workflow types`);

  // Check if service is available
  const healthCheck = http.get(`${STEPFLOW_URL}/api/v1/health`);
  if (healthCheck.status !== 200) {
    throw new Error(`Stepflow service is not available at ${STEPFLOW_URL}`);
  }

  console.log('Stepflow service is healthy...');

  // Store all workflow definitions and get their hashes
  console.log('Storing workflow definitions...');
  const storeFlowParams = {
    headers: {
      'Content-Type': 'application/json',
    },
    timeout: '30s',
  };

  for (const workflow of workflows) {
    const flowPayload = {
      flow: workflowDefinitions[workflow.name]
    };

    console.log(`Storing workflow: ${workflow.description}...`);
    const storeResponse = http.post(`${API_BASE}/flows`, JSON.stringify(flowPayload), storeFlowParams);

    if (storeResponse.status !== 200) {
      throw new Error(`Failed to store workflow ${workflow.name}: ${storeResponse.status} - ${storeResponse.body}`);
    }

    const storeResult = JSON.parse(storeResponse.body);
    if (!storeResult.flowHash) {
      throw new Error(`No flowHash returned for workflow ${workflow.name}: ${storeResponse.body}`);
    }

    flowHashes[workflow.name] = storeResult.flowHash;
    console.log(`✓ Stored ${workflow.description} with hash: ${storeResult.flowHash.substring(0, 8)}...`);
  }

  console.log('All workflows stored successfully, starting load test...');
  return { flowHashes };
}

export default function (data) {
  const workflow = getRandomWorkflow();
  const prompt = getRandomPrompt();
  const flowHash = data.flowHashes[workflow.name];

  if (!flowHash) {
    console.error(`No flow hash found for workflow: ${workflow.name}`);
    failureRate.add(1);
    return;
  }

  const runPayload = {
    flowHash: flowHash,
    input: {
      prompt: prompt.prompt,
      system_message: prompt.system_message
    }
  };

  const params = {
    headers: {
      'Content-Type': 'application/json',
    },
    timeout: '60s', // 60 second timeout for OpenAI calls
  };

  const startTime = Date.now();

  // Execute workflow run
  const response = http.post(`${API_BASE}/runs`, JSON.stringify(runPayload), params);

  const endTime = Date.now();
  const duration = endTime - startTime;

  // Record metrics - Overall
  responseTime.add(duration);
  executeRunTime.add(duration);
  
  // Record metrics - Per workflow type
  if (workflow.name === 'python-udf-openai') {
    pythonUdfResponseTime.add(duration);
    pythonUdfRequests.add(1);
  } else if (workflow.name === 'python-custom-openai') {
    pythonCustomResponseTime.add(duration);
    pythonCustomRequests.add(1);
  } else if (workflow.name === 'rust-builtin-openai') {
    rustBuiltinResponseTime.add(duration);
    rustBuiltinRequests.add(1);
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
        // If status is completed, should have result with response
        if (body.status === 'completed') {
          return body.result && body.result.result.response !== undefined;
        }
        return true; // If not completed, this check passes
      } catch (e) {
        return false;
      }
    }
  });

  // Record failure rates - Overall and per workflow type
  failureRate.add(!success);
  
  if (workflow.name === 'python-udf-openai') {
    pythonUdfFailureRate.add(!success);
  } else if (workflow.name === 'python-custom-openai') {
    pythonCustomFailureRate.add(!success);
  } else if (workflow.name === 'rust-builtin-openai') {
    rustBuiltinFailureRate.add(!success);
  }

  if (!success) {
    console.error(`Request failed for ${workflow.description}`);
    console.error(`Status: ${response.status}`);
    console.error(`Response: ${response.body}`);
  } else {
    const result = JSON.parse(response.body);
    console.log(`✓ ${workflow.description} completed in ${duration}ms (status: ${result.status})`);
  }

  // Brief pause between requests
  sleep(1);
}

export function teardown(data) {
  console.log('Load test completed');
  console.log(`Used flow hashes: ${Object.keys(data.flowHashes).length}`);
}