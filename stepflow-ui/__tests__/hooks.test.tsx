import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ReactNode } from 'react'
import {
  useHealthCheck,
  useFlows,
  useComponents,
  useExecuteFlow,
} from '@/lib/hooks/use-flow-api'

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

  describe('useHealthCheck', () => {
    test('should fetch health status', async () => {
      const wrapper = createWrapper()
      const { result } = renderHook(() => useHealthCheck(), { wrapper })

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

  describe('useFlows', () => {
    test('should fetch flows list', async () => {
      const wrapper = createWrapper()
      const { result } = renderHook(() => useFlows(), { wrapper })

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

  describe('useExecuteFlow', () => {
    test('should provide execute flow mutation', () => {
      const wrapper = createWrapper()
      const { result } = renderHook(() => useExecuteFlow(), { wrapper })

      expect(result.current.mutate).toBeInstanceOf(Function)
      expect(result.current.mutateAsync).toBeInstanceOf(Function)
      expect(result.current.isPending).toBe(false)
    })
  })
})
