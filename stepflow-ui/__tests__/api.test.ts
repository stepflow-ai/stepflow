import { apiClient } from '@/lib/api'
import type { 
  ExecutionDetails, 
  EndpointSummary, 
  ComponentInfo, 
  Workflow 
} from '@/lib/api'

// Test workflow for endpoint creation and execution
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
  let createdEndpointName: string
  let executionId: string

  beforeAll(() => {
    // Generate unique endpoint name for this test run
    createdEndpointName = `test-endpoint-${Date.now()}`
  })

  afterAll(async () => {
    // Cleanup: Delete test endpoint if it was created
    if (createdEndpointName) {
      try {
        await apiClient.deleteEndpoint(createdEndpointName)
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

  describe('Endpoints API', () => {
    test('should list endpoints (initially empty or existing)', async () => {
      const response = await apiClient.listEndpoints()
      
      expect(response).toHaveProperty('endpoints')
      expect(Array.isArray(response.endpoints)).toBe(true)
    })

    test('should create a new endpoint', async () => {
      const endpoint = await apiClient.createEndpoint(createdEndpointName, {
        workflow: testWorkflow
      })
      
      expect(endpoint).toHaveProperty('name', createdEndpointName)
      expect(endpoint).toHaveProperty('workflow_hash')
      expect(endpoint).toHaveProperty('created_at')
      expect(endpoint).toHaveProperty('updated_at')
      expect(typeof endpoint.workflow_hash).toBe('string')
    })

    test('should create an endpoint with label', async () => {
      const labeledEndpointName = `${createdEndpointName}-labeled`
      const endpoint = await apiClient.createEndpoint(labeledEndpointName, {
        workflow: testWorkflow
      }, 'v1.0')
      
      expect(endpoint).toHaveProperty('name', labeledEndpointName)
      expect(endpoint).toHaveProperty('label', 'v1.0')
      expect(endpoint).toHaveProperty('workflow_hash')
      
      // Cleanup the labeled endpoint
      await apiClient.deleteEndpoint(labeledEndpointName, 'v1.0')
    })

    test('should get endpoint details', async () => {
      const endpoint = await apiClient.getEndpoint(createdEndpointName)
      
      expect(endpoint).toHaveProperty('name', createdEndpointName)
      expect(endpoint).toHaveProperty('workflow_hash')
      expect(endpoint).toHaveProperty('created_at')
      expect(endpoint).toHaveProperty('updated_at')
    })

    test('should get endpoint workflow', async () => {
      const response = await apiClient.getEndpointWorkflow(createdEndpointName)
      
      expect(response).toHaveProperty('workflow')
      expect(response).toHaveProperty('workflow_hash')
      expect(response.workflow).toHaveProperty('steps')
      expect(response.workflow.name).toBe(testWorkflow.name)
    })

    test('should execute endpoint', async () => {
      const result = await apiClient.executeEndpoint(createdEndpointName, {
        input: testInput,
        debug: false
      })
      
      expect(result).toHaveProperty('execution_id')
      expect(result).toHaveProperty('status')
      expect(typeof result.execution_id).toBe('string')
    })

    test('should delete endpoint', async () => {
      const result = await apiClient.deleteEndpoint(createdEndpointName)
      
      expect(result).toHaveProperty('message')
      expect(typeof result.message).toBe('string')
      
      // Verify endpoint is deleted by trying to get it (should fail)
      await expect(apiClient.getEndpoint(createdEndpointName))
        .rejects.toThrow()
      
      // Clear the endpoint name so cleanup doesn't try to delete it again
      createdEndpointName = ''
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

    test('should handle non-existent endpoint', async () => {
      await expect(apiClient.getEndpoint('non-existent-endpoint'))
        .rejects.toThrow()
    })

    test('should handle non-existent execution', async () => {
      await expect(apiClient.getExecution('non-existent-execution-id'))
        .rejects.toThrow()
    })
  })
})