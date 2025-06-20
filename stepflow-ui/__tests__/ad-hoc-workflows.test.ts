import {
  ExecuteAdHocWorkflowRequestSchema,
  ExecuteWorkflowResponseSchema,
  type ExecuteAdHocWorkflowRequest,
  type ExecuteWorkflowResponse,
} from '@/lib/api-types'

describe('Ad-hoc Workflow Support', () => {
  describe('ExecuteAdHocWorkflowRequestSchema', () => {
    it('should validate a valid ad-hoc workflow request', () => {
      const validRequest = {
        workflow: {
          name: 'test-workflow',
          steps: {
            step1: {
              component: 'test-component',
              input: { message: 'hello' }
            }
          }
        },
        input: {
          testInput: 'value'
        },
        debug: false
      }

      const result = ExecuteAdHocWorkflowRequestSchema.safeParse(validRequest)
      expect(result.success).toBe(true)
      
      if (result.success) {
        expect(result.data.workflow).toEqual(validRequest.workflow)
        expect(result.data.input).toEqual(validRequest.input)
        expect(result.data.debug).toBe(false)
      }
    })

    it('should set debug to false by default', () => {
      const requestWithoutDebug = {
        workflow: {
          name: 'test-workflow',
          steps: {}
        },
        input: {}
      }

      const result = ExecuteAdHocWorkflowRequestSchema.safeParse(requestWithoutDebug)
      expect(result.success).toBe(true)
      
      if (result.success) {
        expect(result.data.debug).toBe(false)
      }
    })

    it('should reject request without workflow', () => {
      const invalidRequest = {
        input: { test: 'value' }
      }

      const result = ExecuteAdHocWorkflowRequestSchema.safeParse(invalidRequest)
      expect(result.success).toBe(false)
    })

    it('should reject request without input', () => {
      const invalidRequest = {
        workflow: { name: 'test', steps: {} }
      }

      const result = ExecuteAdHocWorkflowRequestSchema.safeParse(invalidRequest)
      expect(result.success).toBe(false)
    })
  })

  describe('ExecuteWorkflowResponseSchema', () => {
    it('should validate response with string workflowName for named workflows', () => {
      const responseWithWorkflowName = {
        runId: 'run-123',
        result: { outcome: 'success', result: { data: 'processed' } },
        status: 'completed',
        debug: false,
        workflowName: 'my-workflow',
        flowHash: 'abc123'
      }

      const result = ExecuteWorkflowResponseSchema.safeParse(responseWithWorkflowName)
      expect(result.success).toBe(true)
      
      if (result.success) {
        expect(result.data.workflowName).toBe('my-workflow')
        expect(result.data.runId).toBe('run-123')
        expect(result.data.status).toBe('completed')
      }
    })

    it('should validate response with null workflowName for ad-hoc workflows', () => {
      const adHocResponse = {
        runId: 'run-456',
        result: { outcome: 'success', result: { processed: true } },
        status: 'completed',
        debug: true,
        workflowName: null, // Ad-hoc workflows don't have names
        flowHash: 'def456'
      }

      const result = ExecuteWorkflowResponseSchema.safeParse(adHocResponse)
      expect(result.success).toBe(true)
      
      if (result.success) {
        expect(result.data.workflowName).toBeNull()
        expect(result.data.runId).toBe('run-456')
        expect(result.data.debug).toBe(true)
      }
    })

    it('should validate response without result field', () => {
      const responseWithoutResult = {
        runId: 'run-789',
        status: 'running',
        debug: false,
        workflowName: null,
        flowHash: 'ghi789'
      }

      const result = ExecuteWorkflowResponseSchema.safeParse(responseWithoutResult)
      expect(result.success).toBe(true)
      
      if (result.success) {
        expect(result.data.result).toBeUndefined()
        expect(result.data.status).toBe('running')
      }
    })

    it('should reject response with invalid status', () => {
      const invalidResponse = {
        runId: 'run-999',
        status: 'invalid-status',
        debug: false,
        workflowName: 'test',
        flowHash: 'invalid123'
      }

      const result = ExecuteWorkflowResponseSchema.safeParse(invalidResponse)
      expect(result.success).toBe(false)
    })

    it('should reject response missing required fields', () => {
      const incompleteResponse = {
        runId: 'run-incomplete'
        // Missing status, debug, workflowName, flowHash
      }

      const result = ExecuteWorkflowResponseSchema.safeParse(incompleteResponse)
      expect(result.success).toBe(false)
    })
  })

  describe('Type Safety', () => {
    it('should provide proper TypeScript types', () => {
      // This test validates that the types compile correctly
      const request: ExecuteAdHocWorkflowRequest = {
        workflow: {
          name: 'typed-workflow',
          steps: {
            step1: {
              component: 'data-processor',
              input: {
                config: { format: 'json' }
              }
            }
          }
        },
        input: {
          data: 'test data',
          options: { validate: true }
        },
        debug: true
      }

      // Verify the structure is as expected
      expect(typeof request.workflow).toBe('object')
      expect(typeof request.input).toBe('object')
      expect(typeof request.debug).toBe('boolean')
    })

    it('should allow nullable workflowName in response types', () => {
      // This test validates TypeScript type compatibility
      const namedWorkflowResponse: ExecuteWorkflowResponse = {
        runId: 'run-001',
        status: 'completed',
        debug: false,
        workflowName: 'my-workflow', // string for named workflows
        flowHash: 'hash001'
      }

      const adHocWorkflowResponse: ExecuteWorkflowResponse = {
        runId: 'run-002',
        status: 'completed',
        debug: false,
        workflowName: null, // null for ad-hoc workflows
        flowHash: 'hash002'
      }

      // These should compile without TypeScript errors
      expect(namedWorkflowResponse.workflowName).toBe('my-workflow')
      expect(adHocWorkflowResponse.workflowName).toBeNull()
    })
  })
})