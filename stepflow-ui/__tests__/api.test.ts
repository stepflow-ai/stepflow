import {
  ComponentApi,
  DebugApi,
  ExecutionApi,
  HealthApi,
  WorkflowApi,
  Configuration
} from '../api-client';
import type { Flow } from '../api-client/models/Flow';
import type { ComponentInfoResponse } from '../api-client/models/ComponentInfoResponse';
import type { ExecutionDetailsResponse } from '../api-client/models/ExecutionDetailsResponse';

const basePath = process.env.STEPFLOW_API_URL || 'http://localhost:7837/api/v1';
const config = new Configuration({ basePath });
const componentApi = new ComponentApi(config);
const debugApi = new DebugApi(config);
const executionApi = new ExecutionApi(config);
const healthApi = new HealthApi(config);
const workflowApi = new WorkflowApi(config);

// Base test workflow that will have its name updated per test run
const baseTestWorkflow: Flow = {
  name: 'Test Math Workflow',
  description: 'A simple test workflow for API integration tests',
  inputSchema: {
    type: 'object',
    properties: {
      a: { type: 'number' },
      b: { type: 'number' }
    },
    required: ['a', 'b']
  },
  steps: [
    {
      id: 'add_numbers',
      component: 'python://add',
      input: {
        a: { $from: { workflow: 'input' }, path: 'a' },
        b: { $from: { workflow: 'input' }, path: 'b' }
      }
    }
  ],
  output: {
    result: {
      $from: { step: 'add_numbers' },
      path: 'result'
    }
  }
};

const testInput = { a: 5, b: 3 }

// Function to create test workflow with the correct name
const createTestWorkflow = (name: string): Flow => ({
  ...baseTestWorkflow,
  name: name
})

describe('StepFlow API Integration Tests', () => {
  let createdWorkflowName: string
  let createdLabel: string
  let executionId: string

  beforeAll(() => {
    // Generate unique workflow name for this test run
    createdWorkflowName = `test-workflow-${Date.now()}`
    createdLabel = 'test-label'
  })

  afterAll(async () => {
    // Cleanup: Delete test labels if they were created
    if (createdWorkflowName && createdLabel) {
      try {
        await workflowApi.deleteLabel({ name: createdWorkflowName, label: createdLabel })
      } catch (error) {
        // Ignore errors during cleanup
        console.warn('Cleanup failed:', error)
      }
    }
  })

  describe('Health Check', () => {
    test('should return healthy status', async () => {
      const health = await healthApi.healthCheck()

      expect(health).toHaveProperty('status', 'healthy')
      expect(health).toHaveProperty('timestamp')
      expect(typeof health.timestamp).toBe('string')
    })
  })

  describe('Components API', () => {
    test('should list available components', async () => {
      const response = await componentApi.listComponents({ includeSchemas: true })

      expect(response).toHaveProperty('components')
      expect(Array.isArray(response.components)).toBe(true)

      if (response.components.length > 0) {
        const component = response.components[0] as ComponentInfoResponse
        expect(component).toHaveProperty('name')
        expect(component).toHaveProperty('pluginName')
        expect(typeof component.name).toBe('string')
        expect(typeof component.pluginName).toBe('string')
      }
    })

    test('should list components without schemas', async () => {
      const response = await componentApi.listComponents({ includeSchemas: false })

      expect(response).toHaveProperty('components')
      expect(Array.isArray(response.components)).toBe(true)
    })
  })

  describe('Ad-hoc Execution', () => {
    test('should execute a workflow directly', async () => {
      const result = await executionApi.createExecution({ createExecutionRequest: { workflow: baseTestWorkflow, input: testInput, debug: false } })

      expect(result).toHaveProperty('executionId')
      expect(result).toHaveProperty('status')
      expect(typeof result.executionId).toBe('string')
      expect(typeof result.status).toBe('string')

      // Store execution ID for later tests
      executionId = result.executionId
    })
  })

  describe('Executions API', () => {
    test('should list executions', async () => {
      const response = await executionApi.listExecutions()

      expect(response).toHaveProperty('executions')
      expect(Array.isArray(response.executions)).toBe(true)

      if (response.executions.length > 0) {
        const execution = response.executions[0]
        expect(execution).toHaveProperty('executionId')
        expect(execution).toHaveProperty('status')
        expect(execution).toHaveProperty('createdAt')
        expect(execution).toHaveProperty('debugMode')
        expect(typeof execution.executionId).toBe('string')
        expect(typeof execution.status).toBe('string')
        expect(typeof execution.debugMode).toBe('boolean')
      }
    })

    test('should get execution details', async () => {
      if (!executionId) {
        // Create a test execution if we don't have one
        const result = await executionApi.createExecution({ createExecutionRequest: { workflow: baseTestWorkflow, input: testInput, debug: false } })
        executionId = result.executionId
      }

      const execution = await executionApi.getExecution({ executionId })

      expect(execution).toHaveProperty('executionId', executionId)
      expect(execution).toHaveProperty('status')
      expect(execution).toHaveProperty('createdAt')
      expect(execution).toHaveProperty('debugMode')
      expect(['pending', 'running', 'completed', 'failed']).toContain(execution.status)
    })

    test('should get execution steps', async () => {
      if (!executionId) {
        const result = await executionApi.createExecution({ createExecutionRequest: { workflow: baseTestWorkflow, input: testInput, debug: false } })
        executionId = result.executionId
      }

      const response = await executionApi.getExecutionSteps({ executionId })

      expect(response).toHaveProperty('steps')
      expect(Array.isArray(response.steps)).toBe(true)
    })

    test('should get execution workflow', async () => {
      if (!executionId) {
        const result = await executionApi.createExecution({ createExecutionRequest: { workflow: baseTestWorkflow, input: testInput, debug: false } })
        executionId = result.executionId
      }

      const response = await executionApi.getExecutionWorkflow({ executionId })

      expect(response).toHaveProperty('workflow')
      expect(response).toHaveProperty('workflowHash')
      expect(response.workflow).toHaveProperty('steps')
      expect(Array.isArray(response.workflow.steps)).toBe(true)
    })
  })

  describe('Workflow Names and Labels API', () => {
    test('should list workflow names', async () => {
      const response = await workflowApi.listWorkflowNames()

      expect(response).toHaveProperty('names')
      expect(Array.isArray(response.names)).toBe(true)
    })

    test('should store a workflow and create a label', async () => {
      // First store the workflow with the correct name
      const testWorkflow = createTestWorkflow(createdWorkflowName)
      const storeResponse = await workflowApi.storeWorkflow({ storeWorkflowRequest: { workflow: testWorkflow } })
      expect(storeResponse).toHaveProperty('workflowHash')
      expect(typeof storeResponse.workflowHash).toBe('string')

      // Create a label for the workflow
      const label = await workflowApi.createOrUpdateLabel({ name: createdWorkflowName, label: createdLabel, createLabelRequest: { workflowHash: storeResponse.workflowHash } })

      expect(label).toHaveProperty('name', createdWorkflowName)
      expect(label).toHaveProperty('label', createdLabel)
      expect(label).toHaveProperty('workflowHash', storeResponse.workflowHash)
      expect(label).toHaveProperty('createdAt')
      expect(label).toHaveProperty('updatedAt')
    })

    test('should get workflow by label', async () => {
      const response = await workflowApi.getWorkflowByLabel({ name: createdWorkflowName, label: createdLabel })

      expect(response).toHaveProperty('workflow')
      expect(response).toHaveProperty('workflowHash')
      expect(response.workflow).toHaveProperty('steps')
      expect(response.workflow.name).toBe(createdWorkflowName)
    })

    test('should list labels for a workflow name', async () => {
      const response = await workflowApi.listLabelsForName({ name: createdWorkflowName })

      expect(response).toHaveProperty('labels')
      expect(Array.isArray(response.labels)).toBe(true)
      expect(response.labels.length).toBeGreaterThan(0)

      const label = response.labels.find((l: any) => l.label === createdLabel)
      expect(label).toBeDefined()
      expect(label?.name).toBe(createdWorkflowName)
    })

    test('should execute workflow by label', async () => {
      const result = await workflowApi.executeWorkflowByLabel({ name: createdWorkflowName, label: createdLabel, executeWorkflowRequest: { input: testInput, debug: false } })

      expect(result).toHaveProperty('executionId')
      expect(result).toHaveProperty('status')
      expect(typeof result.executionId).toBe('string')
    })

    test('should execute workflow by name (latest)', async () => {
      const result = await workflowApi.executeWorkflowByName({ name: createdWorkflowName, executeWorkflowRequest: { input: testInput, debug: false } })

      expect(result).toHaveProperty('executionId')
      expect(result).toHaveProperty('status')
      expect(typeof result.executionId).toBe('string')
    })

    test('should get latest workflow by name', async () => {
      const response = await workflowApi.getLatestWorkflowByName({ name: createdWorkflowName })

      expect(response).toHaveProperty('workflow')
      expect(response).toHaveProperty('workflowHash')
      expect(response.workflow).toHaveProperty('steps')
      expect(response.workflow.name).toBe(createdWorkflowName)
    })

    test('should get workflows by name', async () => {
      const response = await workflowApi.getWorkflowsByName({ name: createdWorkflowName })

      expect(response).toHaveProperty('name', createdWorkflowName)
      expect(response).toHaveProperty('workflows')
      expect(Array.isArray(response.workflows)).toBe(true)
      expect(response.workflows.length).toBeGreaterThan(0)
    })

    test('should delete label', async () => {
      await workflowApi.deleteLabel({ name: createdWorkflowName, label: createdLabel })
      await expect(workflowApi.getWorkflowByLabel({ name: createdWorkflowName, label: createdLabel }))
        .rejects.toThrow()
      createdLabel = ''
    })
  })

  describe('Debug Mode Execution', () => {
    test('should execute workflow in debug mode', async () => {
      const result = await executionApi.createExecution({ createExecutionRequest: { workflow: baseTestWorkflow, input: testInput, debug: true } })

      expect(result).toHaveProperty('executionId')
      expect(result).toHaveProperty('status')
    })
  })

  describe('Error Handling', () => {
    test('should handle empty workflow execution', async () => {
      const emptyWorkflow = { steps: [] } as Flow

      const result = await executionApi.createExecution({ createExecutionRequest: { workflow: emptyWorkflow, input: {}, debug: false } })
      
      expect(result).toHaveProperty('executionId')
      expect(result.status).toBe('completed')
    })

    test('should handle non-existent workflow name', async () => {
      await expect(workflowApi.getLatestWorkflowByName({ name: 'non-existent-workflow' }))
        .rejects.toThrow()
    })

    test('should handle non-existent workflow label', async () => {
      await expect(workflowApi.getWorkflowByLabel({ name: 'non-existent-workflow', label: 'non-existent-label' }))
        .rejects.toThrow()
    })

    test('should handle non-existent execution', async () => {
      await expect(executionApi.getExecution({ executionId: 'non-existent-execution-id' }))
        .rejects.toThrow()
    })
  })
})