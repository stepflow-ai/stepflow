/**
 * Basic API connectivity tests
 *
 * These tests verify that the API client can connect to the StepFlow server
 * and that the basic endpoints work correctly. They serve as integration
 * tests between the frontend TypeScript client and the Rust backend.
 *
 * To run these tests:
 * 1. Start the StepFlow server: cargo run -- serve --port 7837 --config tests/stepflow-config.yml
 * 2. Run the tests: npm test -- --testPathPattern=api-basic.test.ts
 */

import {
  ComponentApi,
  HealthApi,
  ExecutionApi,
  WorkflowApi,
  Configuration
} from '../api-client';

const basePath = process.env.STEPFLOW_API_URL || 'http://localhost:7837/api/v1';
const config = new Configuration({ basePath });
const componentsApi = new ComponentApi(config);
const healthApi = new HealthApi(config);
const executionsApi = new ExecutionApi(config);
const workflowsApi = new WorkflowApi(config);

describe('Basic API Connectivity', () => {
  // Skip tests if server is not running to avoid failing in CI
  const isServerRunning = async () => {
    try {
      await healthApi.healthCheck();
      return true;
    } catch {
      return false;
    }
  }

  test('should connect to health endpoint', async () => {
    const serverRunning = await isServerRunning()

    if (!serverRunning) {
      console.warn(`BASE PATH: ${basePath}`);
      console.warn('⚠️  StepFlow server not running on localhost:7837. Skipping test.')
      console.warn('   Start server with: cargo run -- serve --port 7837 --config tests/stepflow-config.yml')
      return
    }

    const health = await healthApi.healthCheck()

    expect(health).toHaveProperty('status', 'healthy')
    expect(health).toHaveProperty('timestamp')
    expect(typeof health.timestamp).toBe('string')
  })

  test('should list components', async () => {
    const serverRunning = await isServerRunning()
    if (!serverRunning) {
      console.warn('⚠️  StepFlow server not running. Skipping test.')
      return
    }

    const response = await componentsApi.listComponents({})

    expect(response).toHaveProperty('components')
    expect(Array.isArray(response.components)).toBe(true)

    // Should have at least the builtins
    expect(response.components.length).toBeGreaterThan(0)

    const builtin = response.components.find(c => c.pluginName === 'builtins')
    expect(builtin).toBeDefined()
  })

  test('should list executions (empty initially)', async () => {
    const serverRunning = await isServerRunning()
    if (!serverRunning) {
      console.warn('⚠️  StepFlow server not running. Skipping test.')
      return
    }

    const response = await executionsApi.listExecutions({})

    expect(response).toHaveProperty('executions')
    expect(Array.isArray(response.executions)).toBe(true)
  })

  test('should list workflow names (empty initially)', async () => {
    const serverRunning = await isServerRunning()
    if (!serverRunning) {
      console.warn('⚠️  StepFlow server not running. Skipping test.')
      return
    }

    const response = await workflowsApi.listWorkflowNames()

    expect(response).toHaveProperty('names')
    expect(Array.isArray(response.names)).toBe(true)
  })
})