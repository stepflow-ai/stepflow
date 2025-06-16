import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ReactNode } from 'react'
import {
  useHealth,
  useExecutions,
  useComponents,
  useExecuteWorkflow,
  transformStepsForVisualizer
} from '@/lib/hooks/use-api'
import type { StepExecution, Workflow } from '@/lib/api'

// Create a test wrapper for React Query
const createWrapper = () => {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: {
        retry: false,
        gcTime: 0,
      },
    },
  })

  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      {children}
    </QueryClientProvider>
  )
}

describe('React Query Hooks', () => {
  beforeEach(() => {
    // Reset any mock states between tests
    jest.clearAllMocks()
  })

  describe('useHealth', () => {
    test('should fetch health status', async () => {
      const wrapper = createWrapper()
      const { result } = renderHook(() => useHealth(), { wrapper })

      // Initially loading
      expect(result.current.isLoading).toBe(true)
      expect(result.current.data).toBeUndefined()

      // Wait for the query to complete
      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
      }, { timeout: 10000 })

      // Should have health data or error
      if (result.current.isSuccess) {
        expect(result.current.data).toHaveProperty('status')
        expect(result.current.data).toHaveProperty('timestamp')
      } else {
        expect(result.current.isError).toBe(true)
      }
    })
  })

  describe('useExecutions', () => {
    test('should fetch executions list', async () => {
      const wrapper = createWrapper()
      const { result } = renderHook(() => useExecutions(), { wrapper })

      // Initially loading
      expect(result.current.isLoading).toBe(true)

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
      }, { timeout: 10000 })

      if (result.current.isSuccess) {
        expect(Array.isArray(result.current.data)).toBe(true)
      } else {
        expect(result.current.isError).toBe(true)
      }
    })
  })


  describe('useComponents', () => {
    test('should fetch components list', async () => {
      const wrapper = createWrapper()
      const { result } = renderHook(() => useComponents(), { wrapper })

      expect(result.current.isLoading).toBe(true)

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
      }, { timeout: 10000 })

      if (result.current.isSuccess) {
        expect(Array.isArray(result.current.data)).toBe(true)
      } else {
        expect(result.current.isError).toBe(true)
      }
    })

    test('should fetch components without schemas', async () => {
      const wrapper = createWrapper()
      const { result } = renderHook(() => useComponents(false), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
      }, { timeout: 10000 })

      if (result.current.isSuccess) {
        expect(Array.isArray(result.current.data)).toBe(true)
      } else {
        expect(result.current.isError).toBe(true)
      }
    })
  })

  describe('useExecuteWorkflow', () => {
    test('should provide execute workflow mutation', () => {
      const wrapper = createWrapper()
      const { result } = renderHook(() => useExecuteWorkflow(), { wrapper })

      expect(result.current.mutate).toBeInstanceOf(Function)
      expect(result.current.mutateAsync).toBeInstanceOf(Function)
      expect(result.current.isPending).toBe(false)
    })
  })
})

describe('Data Transformation Utilities', () => {
  describe('transformStepsForVisualizer', () => {
    const mockWorkflow: Workflow = {
      steps: [
        {
          id: 'step1',
          component: 'test://component1',
          depends_on: []
        },
        {
          id: 'step2', 
          component: 'test://component2',
          depends_on: ['step1']
        },
        {
          id: 'step3',
          component: 'test://component3',
          depends_on: ['step1', 'step2']
        }
      ]
    }

    test('should transform steps with no execution data', () => {
      const steps: StepExecution[] = []
      const result = transformStepsForVisualizer(steps, mockWorkflow)

      expect(result).toHaveLength(3)
      expect(result[0]).toEqual({
        id: 'step1',
        name: 'step1',
        component: 'test://component1',
        status: 'pending',
        dependencies: [],
        startTime: null,
        duration: null,
        output: null
      })
      expect(result[1]).toEqual({
        id: 'step2',
        name: 'step2', 
        component: 'test://component2',
        status: 'pending',
        dependencies: ['step1'],
        startTime: null,
        duration: null,
        output: null
      })
    })

    test('should transform steps with successful execution', () => {
      const steps: StepExecution[] = [
        {
          step_index: 0,
          step_id: 'step1',
          state: 'completed',
          result: {
            outcome: 'success',
            result: { value: 'result1' }
          }
        },
        {
          step_index: 1,
          step_id: 'step2',
          state: 'completed',
          result: {
            outcome: 'success',
            result: { value: 'result2' }
          }
        }
      ]

      const result = transformStepsForVisualizer(steps, mockWorkflow)

      expect(result[0]).toEqual({
        id: 'step1',
        name: 'step1',
        component: 'test://component1',
        status: 'completed',
        dependencies: [],
        startTime: null,
        duration: null,
        output: '{"value":"result1"}'
      })

      expect(result[1]).toEqual({
        id: 'step2',
        name: 'step2',
        component: 'test://component2', 
        status: 'completed',
        dependencies: ['step1'],
        startTime: null,
        duration: null,
        output: '{"value":"result2"}'
      })
    })

    test('should transform steps with failed execution', () => {
      const steps: StepExecution[] = [
        {
          step_index: 0,
          step_id: 'step1',
          state: 'failed',
          result: {
            outcome: 'failed',
            error: {
              code: 500,
              message: 'Component failed'
            }
          }
        }
      ]

      const result = transformStepsForVisualizer(steps, mockWorkflow)

      expect(result[0]).toEqual({
        id: 'step1',
        name: 'step1',
        component: 'test://component1',
        status: 'failed',
        dependencies: [],
        startTime: null,
        duration: null,
        output: 'Error 500: Component failed'
      })
    })

    test('should transform running steps', () => {
      const steps: StepExecution[] = [
        {
          step_index: 0,
          step_id: 'step1',
          state: 'running'
          // No result means it's running
        }
      ]

      const result = transformStepsForVisualizer(steps, mockWorkflow)

      expect(result[0].status).toBe('running')
      expect(result[0].startTime).toBeNull()
      expect(result[0].duration).toBeNull()
      expect(result[0].output).toBeNull()
    })

    test('should transform skipped steps', () => {
      const steps: StepExecution[] = [
        {
          step_index: 0,
          step_id: 'step1',
          state: 'skipped',
          result: {
            outcome: 'skipped'
          }
        }
      ]

      const result = transformStepsForVisualizer(steps, mockWorkflow)

      expect(result[0].status).toBe('completed') // Skipped is considered completed
      expect(result[0].output).toBe('Skipped')
    })

    test('should handle empty workflow', () => {
      const result = transformStepsForVisualizer([], undefined)
      expect(result).toEqual([])
    })

    test('should handle workflow without steps', () => {
      const result = transformStepsForVisualizer([], { steps: [] })
      expect(result).toEqual([])
    })
  })
})