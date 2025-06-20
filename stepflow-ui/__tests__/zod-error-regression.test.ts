import { ExecuteWorkflowResponseSchema } from '@/lib/api-types'

/**
 * Regression test for the ZodError reported by user:
 * "Expected string, received null" for workflowName field
 * in ad-hoc workflow execution responses.
 */
describe('ZodError Regression Test', () => {
  it('should fix the specific ZodError reported by user for ad-hoc workflows', () => {
    // This is the exact response structure that was causing the ZodError
    const adHocWorkflowResponse = {
      runId: 'run-abc123',
      result: { outcome: 'success', result: { data: 'some output' } },
      status: 'completed',
      debug: false,
      workflowName: null, // This was causing "Expected string, received null"
      flowHash: 'sha256-abc123def456'
    }

    // This should NOT throw a ZodError anymore
    const result = ExecuteWorkflowResponseSchema.safeParse(adHocWorkflowResponse)
    
    // Verify the parse succeeded
    expect(result.success).toBe(true)
    
    if (result.success) {
      expect(result.data.workflowName).toBeNull()
      expect(result.data.runId).toBe('run-abc123')
      expect(result.data.status).toBe('completed')
    }
  })

  it('should still work for named workflows with workflowName as string', () => {
    // Named workflows should continue to work as before
    const namedWorkflowResponse = {
      runId: 'run-def456',
      result: { outcome: 'success', result: { processed: true } },
      status: 'completed',
      debug: true,
      workflowName: 'my-data-pipeline', // String value for named workflows
      flowHash: 'sha256-def456ghi789'
    }

    const result = ExecuteWorkflowResponseSchema.safeParse(namedWorkflowResponse)
    
    expect(result.success).toBe(true)
    
    if (result.success) {
      expect(result.data.workflowName).toBe('my-data-pipeline')
      expect(result.data.runId).toBe('run-def456')
      expect(result.data.debug).toBe(true)
    }
  })
})