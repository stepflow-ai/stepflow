// Mock the UI API client first (must be hoisted)
jest.mock('@/lib/ui-api-client', () => ({
  uiApi: {
    listFlows: jest.fn(),
    getFlow: jest.fn(),
    getRun: jest.fn(),
    getRunSteps: jest.fn(),
    getRunWorkflow: jest.fn(),
    listComponents: jest.fn(),
    executeFlow: jest.fn(),
    executeAdHocWorkflow: jest.fn(),
    healthCheck: jest.fn(),
  },
}))

import { renderHook, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ReactNode } from 'react'
import {
  useHealthCheck,
  useFlows,
  useFlow,
  useRun,
  useRunSteps,
  useRunWorkflow,
  useComponents,
  useExecuteFlow,
  useExecuteAdHocWorkflow,
} from '@/lib/hooks/use-flow-api'

// Import the mocked uiApi after mocking
import { uiApi } from '@/lib/ui-api-client'
const mockUiApi = uiApi as jest.Mocked<typeof uiApi>

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

describe('API Hooks', () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  describe('useHealthCheck', () => {
    it('should fetch health status successfully', async () => {
      const mockHealthData = {
        status: 'healthy',
        timestamp: '2023-01-01T00:00:00Z',
        version: '1.0.0'
      }

      mockUiApi.healthCheck.mockResolvedValue(mockHealthData)

      const wrapper = createWrapper()
      const { result } = renderHook(() => useHealthCheck(), { wrapper })

      expect(result.current.isLoading).toBe(true)

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isSuccess).toBe(true)
        expect(result.current.data).toEqual(mockHealthData)
      })

      expect(mockUiApi.healthCheck).toHaveBeenCalledTimes(1)
    })

    it('should handle health check errors', async () => {
      const mockError = new Error('Health check failed')
      mockUiApi.healthCheck.mockRejectedValue(mockError)

      const wrapper = createWrapper()
      const { result } = renderHook(() => useHealthCheck(), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isError).toBe(true)
        expect(result.current.error).toEqual(mockError)
      })
    })
  })

  describe('useFlows', () => {
    it('should fetch flows list successfully', async () => {
      const mockFlowsData = [
        { 
          id: 1,
          name: 'workflow1', 
          description: null,
          flowHash: 'hash1',
          createdAt: '2024-01-15T10:30:00Z',
          updatedAt: '2024-01-15T10:30:00Z',
          labelCount: 0,
          executionCount: 5
        },
        { 
          id: 2,
          name: 'workflow2', 
          description: 'Test workflow 2',
          flowHash: 'hash2',
          createdAt: '2024-01-15T11:00:00Z',
          updatedAt: '2024-01-15T11:00:00Z',
          labelCount: 2,
          executionCount: 10
        }
      ]

      mockUiApi.listFlows.mockResolvedValue({ workflows: mockFlowsData })

      const wrapper = createWrapper()
      const { result } = renderHook(() => useFlows(), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isSuccess).toBe(true)
        expect(result.current.data).toEqual(mockFlowsData)
      })

      expect(mockUiApi.listFlows).toHaveBeenCalledTimes(1)
    })

    it('should handle flows list errors', async () => {
      const mockError = new Error('Failed to fetch flows')
      mockUiApi.listFlows.mockRejectedValue(mockError)

      const wrapper = createWrapper()
      const { result } = renderHook(() => useFlows(), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isError).toBe(true)
        expect(result.current.error).toEqual(mockError)
      })
    })
  })

  describe('useFlow', () => {
    it('should fetch flow by name successfully', async () => {
      const mockFlowData = {
        id: 1,
        name: 'test-workflow',
        description: 'Test workflow description',
        flowHash: 'sha256-test123',
        createdAt: '2024-01-15T10:30:00Z',
        updatedAt: '2024-01-15T10:30:00Z',
        labels: [
          {
            label: 'latest',
            flowHash: 'sha256-test123',
            createdAt: '2024-01-15T10:30:00Z',
            updatedAt: '2024-01-15T10:30:00Z'
          }
        ],
        recentExecutions: [
          {
            id: 'run-123',
            status: 'completed' as const,
            debug: false,
            createdAt: '2024-01-15T10:30:00Z',
            completedAt: '2024-01-15T10:35:00Z'
          }
        ],
        flow: {
          name: 'test-workflow',
          steps: {
            step1: { component: 'test-component', input: {} }
          }
        },
        analysis: {
          dependencies: []
        }
      }

      mockUiApi.getFlow.mockResolvedValue(mockFlowData)

      const wrapper = createWrapper()
      const { result } = renderHook(() => useFlow('test-workflow'), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isSuccess).toBe(true)
        expect(result.current.data).toEqual(mockFlowData)
      })

      expect(mockUiApi.getFlow).toHaveBeenCalledWith('test-workflow')
    })

    it('should not fetch when name is empty', () => {
      const wrapper = createWrapper()
      const { result } = renderHook(() => useFlow(''), { wrapper })

      expect(result.current.isLoading).toBe(false)
      expect(result.current.data).toBeUndefined()
      expect(mockUiApi.getFlow).not.toHaveBeenCalled()
    })
  })

  describe('useRun', () => {
    it('should fetch run details successfully', async () => {
      const mockRunData = {
        runId: 'run123',
        flowName: 'test-workflow',
        flowLabel: 'latest',
        flowHash: 'sha256-abc123',
        status: 'completed' as const,
        debugMode: false,
        createdAt: '2023-01-01T00:00:00Z',
        completedAt: '2023-01-01T00:01:00Z',
        input: { test: 'data' },
        result: { outcome: 'success' as const, result: { processed: true } }
      }

      mockUiApi.getRun.mockResolvedValue(mockRunData)

      const wrapper = createWrapper()
      const { result } = renderHook(() => useRun('run123'), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isSuccess).toBe(true)
        expect(result.current.data).toEqual(mockRunData)
      })

      expect(mockUiApi.getRun).toHaveBeenCalledWith('run123')
    })
  })

  describe('useRunSteps', () => {
    it('should fetch run steps successfully', async () => {
      const mockStepsData = [
        {
          runId: 'run123',
          stepIndex: 0,
          stepId: 'step1',
          state: 'completed' as const,
          startedAt: '2023-01-01T00:00:00Z',
          completedAt: '2023-01-01T00:00:30Z',
          result: { outcome: 'success' as const, result: { data: 'step1 result' } }
        },
        {
          runId: 'run123',
          stepIndex: 1,
          stepId: 'step2',
          state: 'running' as const,
          startedAt: '2023-01-01T00:00:30Z',
          completedAt: null
        }
      ]

      mockUiApi.getRunSteps.mockResolvedValue({ steps: mockStepsData })

      const wrapper = createWrapper()
      const { result } = renderHook(() => useRunSteps('run123'), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isSuccess).toBe(true)
        expect(result.current.data).toEqual(mockStepsData)
      })

      expect(mockUiApi.getRunSteps).toHaveBeenCalledWith('run123')
    })
  })

  describe('useRunWorkflow', () => {
    it('should fetch run workflow successfully', async () => {
      const mockWorkflowData = {
        runId: 'run123',
        flowHash: 'sha256-abc123',
        debugMode: false,
        workflowName: 'test-workflow',
        workflowDescription: 'Test workflow description',
        workflowLabels: [
          {
            label: 'latest',
            flowHash: 'sha256-abc123',
            createdAt: '2024-01-15T10:30:00Z',
            updatedAt: '2024-01-15T10:30:00Z'
          }
        ],
        flow: {
          name: 'test-workflow',
          steps: {
            step1: { component: 'test-component', input: {} }
          }
        }
      }

      mockUiApi.getRunWorkflow.mockResolvedValue(mockWorkflowData)

      const wrapper = createWrapper()
      const { result } = renderHook(() => useRunWorkflow('run123'), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isSuccess).toBe(true)
        expect(result.current.data).toEqual(mockWorkflowData)
      })

      expect(mockUiApi.getRunWorkflow).toHaveBeenCalledWith('run123')
    })
  })

  describe('useComponents', () => {
    it('should fetch components with schemas by default', async () => {
      const mockComponentsData = [
        {
          url: 'test-component',
          name: 'test-component',
          description: 'A test component',
          inputSchema: { type: 'object' },
          outputSchema: { type: 'object' }
        }
      ]

      mockUiApi.listComponents.mockResolvedValue({ components: mockComponentsData })

      const wrapper = createWrapper()
      const { result } = renderHook(() => useComponents(), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isSuccess).toBe(true)
        expect(result.current.data).toEqual(mockComponentsData)
      })

      expect(mockUiApi.listComponents).toHaveBeenCalledWith(true)
    })

    it('should fetch components without schemas when specified', async () => {
      const mockComponentsData = [
        {
          url: 'test-component',
          name: 'test-component',
          description: 'A test component'
        }
      ]

      mockUiApi.listComponents.mockResolvedValue({ components: mockComponentsData })

      const wrapper = createWrapper()
      const { result } = renderHook(() => useComponents(false), { wrapper })

      await waitFor(() => {
        expect(result.current.isLoading).toBe(false)
        expect(result.current.isSuccess).toBe(true)
        expect(result.current.data).toEqual(mockComponentsData)
      })

      expect(mockUiApi.listComponents).toHaveBeenCalledWith(false)
    })
  })

  describe('useExecuteFlow', () => {
    it('should provide execute flow mutation', async () => {
      const mockExecuteResponse = {
        runId: 'run123',
        status: 'running' as const,
        debug: false,
        workflowName: 'test-flow',
        flowHash: 'sha256-abc123'
      }

      mockUiApi.executeFlow.mockResolvedValue(mockExecuteResponse)

      const wrapper = createWrapper()
      const { result } = renderHook(() => useExecuteFlow(), { wrapper })

      expect(result.current.mutate).toBeInstanceOf(Function)
      expect(result.current.mutateAsync).toBeInstanceOf(Function)
      expect(result.current.isPending).toBe(false)

      // Test mutation execution
      result.current.mutate({
        name: 'test-flow',
        input: { test: 'data' },
        debug: false
      })

      await waitFor(() => {
        expect(mockUiApi.executeFlow).toHaveBeenCalledWith('test-flow', { input: { test: 'data' }, debug: false })
      })
    })

    it('should handle execute flow errors', async () => {
      const mockError = new Error('Execution failed')
      mockUiApi.executeFlow.mockRejectedValue(mockError)

      const wrapper = createWrapper()
      const { result } = renderHook(() => useExecuteFlow(), { wrapper })

      result.current.mutate({
        name: 'test-flow',
        input: { test: 'data' }
      })

      await waitFor(() => {
        expect(result.current.isError).toBe(true)
        expect(result.current.error).toEqual(mockError)
      })
    })
  })
})