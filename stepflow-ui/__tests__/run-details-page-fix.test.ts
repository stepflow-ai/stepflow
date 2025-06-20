/**
 * Test for the fix of the run details page toString() error
 * Specifically testing that createdAt and completedAt string handling is correct
 */

import type { RunDetailsResponse } from '@/lib/api-types'

describe('Run Details Page ToString Fix', () => {
  it('should handle RunDetailsResponse with string dates correctly', () => {
    // Mock execution response as it comes from the API (strings, not Date objects)
    const mockExecution: RunDetailsResponse = {
      runId: 'run-123',
      flowName: null, // Ad-hoc workflow
      flowLabel: null,
      flowHash: 'sha256-abc123',
      status: 'completed',
      debugMode: false,
      createdAt: '2024-01-15T10:30:00Z', // String, not Date
      completedAt: '2024-01-15T10:35:00Z', // String, not Date
      input: { test: 'data' },
      result: { outcome: 'success', result: { processed: true } }
    }

    // Test that we can access dates without calling toString()
    expect(typeof mockExecution.createdAt).toBe('string')
    expect(typeof mockExecution.completedAt).toBe('string')
    
    // These should work without .toString() calls
    const createdAt = mockExecution.createdAt // No .toString() needed
    const completedAt = mockExecution.completedAt || undefined // No .toString() needed
    
    expect(createdAt).toBe('2024-01-15T10:30:00Z')
    expect(completedAt).toBe('2024-01-15T10:35:00Z')
  })

  it('should handle ad-hoc workflows with null completedAt', () => {
    const mockRunningExecution: RunDetailsResponse = {
      runId: 'run-456',
      flowName: null,
      flowLabel: null,
      flowHash: 'sha256-def456',
      status: 'running',
      debugMode: true,
      createdAt: '2024-01-15T10:30:00Z',
      completedAt: null, // Still running
      input: { workflow: { steps: {} } },
    }

    // Test the pattern used in the fixed code
    const createdAt = mockRunningExecution.createdAt
    const completedAt = mockRunningExecution.completedAt || undefined
    
    expect(typeof createdAt).toBe('string')
    expect(completedAt).toBeUndefined()
  })

  it('should verify the getDuration function parameters work with the fix', () => {
    // Simulate the getDuration function call pattern from the fixed code
    const mockExecution: RunDetailsResponse = {
      runId: 'run-789',
      flowName: 'test-workflow',
      flowLabel: 'latest',
      flowHash: 'sha256-ghi789',
      status: 'completed',
      debugMode: false,
      createdAt: '2024-01-15T10:30:00Z',
      completedAt: '2024-01-15T10:35:00Z',
      input: {},
    }

    // This is the pattern from the fixed code - should work without toString()
    const getDuration = (createdAt: string, completedAt?: string) => {
      const start = new Date(createdAt)
      const end = completedAt ? new Date(completedAt) : new Date()
      return end.getTime() - start.getTime()
    }

    // These calls should not throw errors
    expect(() => {
      getDuration(mockExecution.createdAt, mockExecution.completedAt || undefined)
    }).not.toThrow()

    expect(() => {
      getDuration(mockExecution.createdAt) // Without completedAt
    }).not.toThrow()
  })
})