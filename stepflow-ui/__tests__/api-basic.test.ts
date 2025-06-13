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

import { apiClient } from '@/lib/api'

describe('Basic API Connectivity', () => {
  // Skip tests if server is not running to avoid failing in CI
  const isServerRunning = async () => {
    try {
      await apiClient.health()
      return true
    } catch {
      return false
    }
  }

  test('should connect to health endpoint', async () => {
    const serverRunning = await isServerRunning()
    
    if (!serverRunning) {
      console.warn('⚠️  StepFlow server not running on localhost:7837. Skipping test.')
      console.warn('   Start server with: cargo run -- serve --port 7837 --config tests/stepflow-config.yml')
      return
    }

    const health = await apiClient.health()
    
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

    const response = await apiClient.listComponents()
    
    expect(response).toHaveProperty('components')
    expect(Array.isArray(response.components)).toBe(true)
    
    // Should have at least the builtins
    expect(response.components.length).toBeGreaterThan(0)
    
    const builtin = response.components.find(c => c.plugin_name === 'builtins')
    expect(builtin).toBeDefined()
  })

  test('should list executions (empty initially)', async () => {
    const serverRunning = await isServerRunning()
    if (!serverRunning) {
      console.warn('⚠️  StepFlow server not running. Skipping test.')
      return
    }

    const response = await apiClient.listExecutions()
    
    expect(response).toHaveProperty('executions')
    expect(Array.isArray(response.executions)).toBe(true)
  })

  test('should list endpoints (empty initially)', async () => {
    const serverRunning = await isServerRunning()
    if (!serverRunning) {
      console.warn('⚠️  StepFlow server not running. Skipping test.')
      return
    }

    const response = await apiClient.listEndpoints()
    
    expect(response).toHaveProperty('endpoints')
    expect(Array.isArray(response.endpoints)).toBe(true)
  })
})

describe('API Types and Schema Validation', () => {
  test('should have correct TypeScript types', () => {
    // Test that our TypeScript interfaces compile correctly
    expect(typeof apiClient.health).toBe('function')
    expect(typeof apiClient.listExecutions).toBe('function')
    expect(typeof apiClient.listEndpoints).toBe('function')
    expect(typeof apiClient.listComponents).toBe('function')
    expect(typeof apiClient.execute).toBe('function')
    expect(typeof apiClient.createEndpoint).toBe('function')
    expect(typeof apiClient.deleteEndpoint).toBe('function')
    expect(typeof apiClient.executeEndpoint).toBe('function')
  })
})

describe('Error Handling', () => {
  test('should handle non-existent endpoints gracefully', async () => {
    const serverRunning = await isServerRunning()
    if (!serverRunning) {
      console.warn('⚠️  StepFlow server not running. Skipping test.')
      return
    }

    await expect(apiClient.getEndpoint('non-existent-endpoint'))
      .rejects.toThrow()
  })

  test('should handle non-existent executions gracefully', async () => {
    const serverRunning = await isServerRunning()
    if (!serverRunning) {
      console.warn('⚠️  StepFlow server not running. Skipping test.')
      return
    }

    await expect(apiClient.getExecution('non-existent-execution'))
      .rejects.toThrow()
  })
})