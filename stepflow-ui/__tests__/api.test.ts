import { apiClient } from '@/lib/api'
import type { 
  ExecutionDetails, 
  ComponentInfo, 
  Workflow 
} from '@/lib/api'

// Test workflow for workflow execution
const testWorkflow: Workflow = {
  name: 'Test Math Workflow',
  description: 'A simple test workflow for API integration tests',
  input_schema: {
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
      component: 'builtins://eval',
      input: {
        expression: 'context.a + context.b',
        context: {
          a: { $from: { workflow: 'input' }, path: 'a' },
          b: { $from: { workflow: 'input' }, path: 'b' }
        }
      }
    }
  ],
  output: {
    result: {
      $from: { step: 'add_numbers' },
      path: 'result'
    }
  }
}

const testInput = { a: 5, b: 3 }

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
        await apiClient.deleteLabel(createdWorkflowName, createdLabel)
      } catch (error) {
        // Ignore errors during cleanup
        console.warn('Cleanup failed:', error)
      }
    }
  })

  describe('Health Check', () => {
    test('should return healthy status', async () => {
      const health = await apiClient.health()
      
      expect(health).toHaveProperty('status', 'healthy')
      expect(health).toHaveProperty('timestamp')
      expect(typeof health.timestamp).toBe('string')
    })
  })

  describe('Components API', () => {
    test('should list available components', async () => {
      const response = await apiClient.listComponents(true)
      
      expect(response).toHaveProperty('components')
      expect(Array.isArray(response.components)).toBe(true)
      
      if (response.components.length > 0) {
        const component = response.components[0] as ComponentInfo
        expect(component).toHaveProperty('name')
        expect(component).toHaveProperty('plugin_name')
        expect(typeof component.name).toBe('string')
        expect(typeof component.plugin_name).toBe('string')
      }
    })

    test('should list components without schemas', async () => {
      const response = await apiClient.listComponents(false)
      
      expect(response).toHaveProperty('components')
      expect(Array.isArray(response.components)).toBe(true)
    })
  })

  describe('Ad-hoc Execution', () => {
    test('should execute a workflow directly', async () => {
      const result = await apiClient.execute({
        workflow: testWorkflow,
        input: testInput,
        debug: false
      })
      
      expect(result).toHaveProperty('execution_id')
      expect(result).toHaveProperty('status')
      expect(typeof result.execution_id).toBe('string')
      expect(typeof result.status).toBe('string')
      
      // Store execution ID for later tests
      executionId = result.execution_id
    })
  })

  describe('Executions API', () => {
    test('should list executions', async () => {
      const response = await apiClient.listExecutions()
      
      expect(response).toHaveProperty('executions')
      expect(Array.isArray(response.executions)).toBe(true)
      
      if (response.executions.length > 0) {
        const execution = response.executions[0]
        expect(execution).toHaveProperty('execution_id')
        expect(execution).toHaveProperty('status')
        expect(execution).toHaveProperty('created_at')
        expect(execution).toHaveProperty('debug_mode')
        expect(typeof execution.execution_id).toBe('string')
        expect(typeof execution.status).toBe('string')
        expect(typeof execution.debug_mode).toBe('boolean')
      }
    })

    test('should get execution details', async () => {
      if (!executionId) {
        // Create a test execution if we don't have one
        const result = await apiClient.execute({
          workflow: testWorkflow,
          input: testInput,
          debug: false
        })
        executionId = result.execution_id
      }

      const execution = await apiClient.getExecution(executionId)
      
      expect(execution).toHaveProperty('execution_id', executionId)
      expect(execution).toHaveProperty('status')
      expect(execution).toHaveProperty('created_at')
      expect(execution).toHaveProperty('debug_mode')
      expect(['Pending', 'Running', 'Completed', 'Failed']).toContain(execution.status)
    })

    test('should get execution steps', async () => {
      if (!executionId) {
        const result = await apiClient.execute({
          workflow: testWorkflow,
          input: testInput,
          debug: false
        })
        executionId = result.execution_id
      }

      const response = await apiClient.getExecutionSteps(executionId)
      
      expect(response).toHaveProperty('steps')
      expect(Array.isArray(response.steps)).toBe(true)
    })

    test('should get execution workflow', async () => {
      if (!executionId) {
        const result = await apiClient.execute({
          workflow: testWorkflow,
          input: testInput,
          debug: false
        })
        executionId = result.execution_id
      }

      const response = await apiClient.getExecutionWorkflow(executionId)
      
      expect(response).toHaveProperty('workflow')
      expect(response).toHaveProperty('workflow_hash')
      expect(response.workflow).toHaveProperty('steps')
      expect(Array.isArray(response.workflow.steps)).toBe(true)
    })
  })

  describe('Workflow Names and Labels API', () => {
    test('should list workflow names', async () => {
      const response = await apiClient.listWorkflowNames()
      
      expect(response).toHaveProperty('names')
      expect(Array.isArray(response.names)).toBe(true)
    })

    test('should store a workflow and create a label', async () => {
      // First store the workflow
      const storeResponse = await apiClient.storeWorkflow(testWorkflow)
      expect(storeResponse).toHaveProperty('workflow_hash')
      expect(typeof storeResponse.workflow_hash).toBe('string')
      
      // Create a label for the workflow
      const label = await apiClient.createOrUpdateLabel(createdWorkflowName, createdLabel, storeResponse.workflow_hash)
      
      expect(label).toHaveProperty('name', createdWorkflowName)
      expect(label).toHaveProperty('label', createdLabel)
      expect(label).toHaveProperty('workflow_hash', storeResponse.workflow_hash)
      expect(label).toHaveProperty('created_at')
      expect(label).toHaveProperty('updated_at')
    })

    test('should get workflow by label', async () => {
      const response = await apiClient.getWorkflowByLabel(createdWorkflowName, createdLabel)
      
      expect(response).toHaveProperty('workflow')
      expect(response).toHaveProperty('workflow_hash')
      expect(response.workflow).toHaveProperty('steps')
      expect(response.workflow.name).toBe(testWorkflow.name)
    })

    test('should list labels for a workflow name', async () => {
      const response = await apiClient.listLabelsForName(createdWorkflowName)
      
      expect(response).toHaveProperty('labels')
      expect(Array.isArray(response.labels)).toBe(true)
      expect(response.labels.length).toBeGreaterThan(0)
      
      const label = response.labels.find(l => l.label === createdLabel)
      expect(label).toBeDefined()
      expect(label?.name).toBe(createdWorkflowName)
    })

    test('should execute workflow by label', async () => {
      const result = await apiClient.executeWorkflowByLabel(createdWorkflowName, createdLabel, {
        input: testInput,
        debug: false
      })
      
      expect(result).toHaveProperty('execution_id')
      expect(result).toHaveProperty('status')
      expect(typeof result.execution_id).toBe('string')
    })

    test('should execute workflow by name (latest)', async () => {
      const result = await apiClient.executeWorkflowByName(createdWorkflowName, {
        input: testInput,
        debug: false
      })
      
      expect(result).toHaveProperty('execution_id')
      expect(result).toHaveProperty('status')
      expect(typeof result.execution_id).toBe('string')
    })

    test('should get latest workflow by name', async () => {
      const response = await apiClient.getLatestWorkflowByName(createdWorkflowName)
      
      expect(response).toHaveProperty('workflow')
      expect(response).toHaveProperty('workflow_hash')
      expect(response.workflow).toHaveProperty('steps')
      expect(response.workflow.name).toBe(testWorkflow.name)
    })

    test('should get workflows by name', async () => {
      const response = await apiClient.getWorkflowsByName(createdWorkflowName)
      
      expect(response).toHaveProperty('name', createdWorkflowName)
      expect(response).toHaveProperty('workflows')
      expect(Array.isArray(response.workflows)).toBe(true)
      expect(response.workflows.length).toBeGreaterThan(0)
    })

    test('should delete label', async () => {
      const result = await apiClient.deleteLabel(createdWorkflowName, createdLabel)
      
      expect(result).toHaveProperty('message')
      expect(typeof result.message).toBe('string')
      
      // Verify label is deleted by trying to get it (should fail)
      await expect(apiClient.getWorkflowByLabel(createdWorkflowName, createdLabel))
        .rejects.toThrow()
      
      // Clear the label so cleanup doesn't try to delete it again
      createdLabel = ''
    })
  })

  describe('Debug Mode Execution', () => {
    test('should execute workflow in debug mode', async () => {
      const result = await apiClient.execute({
        workflow: testWorkflow,
        input: testInput,
        debug: true
      })
      
      expect(result).toHaveProperty('execution_id')
      expect(result).toHaveProperty('status')
      
      // In debug mode, we should be able to get debug steps
      // Note: The debug API endpoints may need to be implemented
      // This test verifies the basic debug execution works
    })
  })

  describe('Error Handling', () => {
    test('should handle invalid workflow execution', async () => {
      const invalidWorkflow = {
        steps: [] // Invalid - no steps
      } as Workflow
      
      await expect(apiClient.execute({
        workflow: invalidWorkflow,
        input: {},
        debug: false
      })).rejects.toThrow()
    })

    test('should handle non-existent workflow name', async () => {
      await expect(apiClient.getLatestWorkflowByName('non-existent-workflow'))
        .rejects.toThrow()
    })

    test('should handle non-existent workflow label', async () => {
      await expect(apiClient.getWorkflowByLabel('non-existent-workflow', 'non-existent-label'))
        .rejects.toThrow()
    })

    test('should handle non-existent execution', async () => {
      await expect(apiClient.getExecution('non-existent-execution-id'))
        .rejects.toThrow()
    })
  })
})