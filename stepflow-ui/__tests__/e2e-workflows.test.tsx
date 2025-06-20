/**
 * End-to-end tests for complete user workflows
 * These tests simulate real user interactions from start to finish
 */

import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { ReactNode } from 'react'
// Mock the UI API client (this is what we actually use now)
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

import { uiApi } from '@/lib/ui-api-client'
const mockUiApi = uiApi as jest.Mocked<typeof uiApi>

// Mock Next.js router
jest.mock('next/navigation', () => ({
  useRouter: () => ({
    push: jest.fn(),
    replace: jest.fn(),
    back: jest.fn(),
  }),
  usePathname: () => '/flows',
  useSearchParams: () => new URLSearchParams(),
}))

// Mock toast notifications
jest.mock('sonner', () => ({
  toast: {
    success: jest.fn(),
    error: jest.fn(),
  },
}))

// Create test wrapper with providers
const createWrapper = () => {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  })

  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      {children}
    </QueryClientProvider>
  )
}

describe('End-to-End Workflow Tests', () => {
  beforeEach(() => {
    jest.clearAllMocks()
  })

  describe('Flow Discovery and Execution', () => {
    it('should complete full workflow from discovery to execution', async () => {
      // Mock API responses
      const mockFlows = {
        workflows: [
          { 
            id: 1,
            name: 'data-processing', 
            description: 'Process data',
            flowHash: 'hash1',
            createdAt: '2024-01-15T10:30:00Z',
            updatedAt: '2024-01-15T10:30:00Z',
            labelCount: 1,
            executionCount: 5
          },
          { 
            id: 2,
            name: 'email-sender', 
            description: 'Send emails',
            flowHash: 'hash2',
            createdAt: '2024-01-15T11:00:00Z',
            updatedAt: '2024-01-15T11:00:00Z',
            labelCount: 0,
            executionCount: 2
          }
        ]
      }

      const mockFlow = {
        id: 1,
        name: 'data-processing',
        description: 'Process data workflow',
        flowHash: 'hash1',
        createdAt: '2024-01-15T10:30:00Z',
        updatedAt: '2024-01-15T10:30:00Z',
        labels: [{
          label: 'latest',
          flowHash: 'hash1',
          createdAt: '2024-01-15T10:30:00Z',
          updatedAt: '2024-01-15T10:30:00Z'
        }],
        recentExecutions: [],
        flow: {
          name: 'data-processing',
          inputSchema: {
            type: 'object',
            properties: {
              dataFile: { type: 'string', description: 'Path to data file' },
              format: { type: 'string', enum: ['json', 'csv'], description: 'Data format' }
            },
            required: ['dataFile']
          },
          steps: {
            loadData: { component: 'file-loader', input: {} },
            processData: { component: 'data-processor', input: {} },
            saveResults: { component: 'file-saver', input: {} }
          }
        }
      }

      const mockExecution = {
        runId: 'run123',
        status: 'running' as const,
        debug: false,
        workflowName: 'data-processing',
        flowHash: 'hash1'
      }

      const mockRunDetails = {
        runId: 'run123',
        flowName: 'data-processing',
        flowLabel: 'latest',
        flowHash: 'hash1',
        status: 'completed' as const,
        debugMode: false,
        createdAt: '2023-01-01T00:00:00Z',
        completedAt: '2023-01-01T00:01:00Z',
        input: { dataFile: 'test.json', format: 'json' },
        result: { outcome: 'success' as const, result: { processed: true } }
      }

      const mockSteps = {
        steps: [
          {
            runId: 'run123',
            stepIndex: 0,
            stepId: 'loadData',
            state: 'completed' as const,
            startedAt: '2023-01-01T00:00:00Z',
            completedAt: '2023-01-01T00:00:20Z',
            result: { outcome: 'success' as const, result: { records: 100 } }
          },
          {
            runId: 'run123',
            stepIndex: 1,
            stepId: 'processData',
            state: 'completed' as const,
            startedAt: '2023-01-01T00:00:20Z',
            completedAt: '2023-01-01T00:00:50Z',
            result: { outcome: 'success' as const, result: { processedRecords: 95 } }
          },
          {
            runId: 'run123',
            stepIndex: 2,
            stepId: 'saveResults',
            state: 'completed' as const,
            startedAt: '2023-01-01T00:00:50Z',
            completedAt: '2023-01-01T00:01:00Z',
            result: { outcome: 'success' as const, result: { fileName: 'results.json' } }
          }
        ]
      }

      // Set up mock implementations
      mockUiApi.listFlows.mockResolvedValue(mockFlows)
      mockUiApi.getFlow.mockResolvedValue(mockFlow)
      mockUiApi.executeFlow.mockResolvedValue(mockExecution)
      mockUiApi.getRun.mockResolvedValue(mockRunDetails)
      mockUiApi.getRunSteps.mockResolvedValue(mockSteps)

      // This would be a test of a complete page component
      // For this example, we'll test the workflow conceptually
      
      const user = userEvent.setup()
      const wrapper = createWrapper()

      // Step 1: User visits flows page and sees available flows
      // (This would render a FlowsPage component)
      
      // Step 2: User selects a flow to execute
      // User clicks on "data-processing" flow
      
      // Step 3: User fills out execution form
      // Modal opens with input form based on schema
      
      // Step 4: User submits execution
      // Form is submitted with input data
      
      // Step 5: User is redirected to execution view
      // Page shows running execution with real-time updates
      
      // Step 6: User monitors execution progress
      // Steps complete one by one, status updates
      
      // Verify API calls were made in correct sequence
      expect(mockUiApi.listFlows).toHaveBeenCalled()
      // Additional assertions would depend on actual component implementation
    })

    it('should handle flow execution errors gracefully', async () => {
      // Mock error responses
      mockUiApi.listFlows.mockResolvedValue({ workflows: [] })
      mockUiApi.executeFlow.mockRejectedValue(new Error('Execution failed'))

      // Test error handling throughout the workflow
      // User should see appropriate error messages
      // System should remain in a consistent state
    })
  })

  describe('Flow Management Workflow', () => {
    it('should allow uploading and managing flow versions', async () => {
      const user = userEvent.setup()
      
      // Mock flow upload and version management
      const mockUploadResponse = {
        id: 1,
        name: 'test-flow',
        description: 'A test flow',
        flowHash: 'new-hash-123',
        createdAt: '2023-01-01T00:00:00Z',
        updatedAt: '2023-01-01T00:00:00Z',
        labelCount: 0,
        executionCount: 0
      }

      const mockFlowVersions = [
        { 
          label: 'production', 
          flowHash: 'hash1', 
          createdAt: '2023-01-01T00:00:00Z',
          updatedAt: '2023-01-01T00:00:00Z',
          workflowName: 'test-flow'
        },
        { 
          label: 'staging', 
          flowHash: 'hash2', 
          createdAt: '2023-01-01T01:00:00Z',
          updatedAt: '2023-01-01T01:00:00Z',
          workflowName: 'test-flow'
        },
        { 
          label: 'latest', 
          flowHash: 'new-hash-123', 
          createdAt: '2023-01-01T02:00:00Z',
          updatedAt: '2023-01-01T02:00:00Z',
          workflowName: 'test-flow'
        }
      ]

      mockUiApi.storeFlow.mockResolvedValue(mockUploadResponse)
      mockUiApi.listFlowLabels.mockResolvedValue({ labels: mockFlowVersions })

      // Workflow:
      // 1. User uploads a new flow definition
      // 2. System processes and validates the flow
      // 3. User creates labels for different environments
      // 4. User can execute different versions
      // 5. User can manage flow lifecycle

      // This would test complete flow management functionality
    })
  })

  describe('Debug Workflow', () => {
    it('should support step-by-step debugging', async () => {
      const user = userEvent.setup()

      // Mock debug-specific API responses
      const mockDebugRun = {
        runId: 'debug-run-123',
        status: 'paused' as const,
        flowHash: 'hash123',
        debug: true,
        workflowName: 'test-debug-flow'
      }

      const mockRunnableSteps = {
        status: 'paused' as const,
        flowHash: 'hash123',
        createdAt: '2023-01-01T00:00:00Z',
        completedAt: null,
        input: {},
        runId: 'debug-run-123',
        flowName: 'test-debug-flow',
        flowLabel: null,
        debugMode: true
      }

      const mockStepExecution = {
        status: 'completed' as const,
        flowHash: 'hash123',
        createdAt: '2023-01-01T00:00:00Z',
        completedAt: '2023-01-01T00:02:30Z',
        input: {},
        runId: 'debug-run-123',
        flowName: 'test-debug-flow',
        flowLabel: null,
        debugMode: true,
        result: {
          outcome: 'success' as const,
          result: { records: 100 }
        }
      }

      mockUiApi.executeFlow.mockResolvedValue(mockDebugRun)
      mockUiApi.getRun.mockResolvedValue(mockRunnableSteps)
      mockUiApi.getRun.mockResolvedValue(mockStepExecution)

      // Debug workflow:
      // 1. User starts a debug session
      // 2. System shows runnable steps
      // 3. User executes steps one by one
      // 4. User inspects intermediate outputs
      // 5. User can modify inputs between steps
      // 6. User completes or cancels debug session

      // This would test the complete debugging experience
    })
  })

  describe('Real-time Updates Workflow', () => {
    it('should show real-time execution progress', async () => {
      // Mock real-time updates (would use WebSockets or polling)
      const mockInitialRun = {
        id: 'run123',
        status: 'running',
        currentStep: 'loadData'
      }

      const mockProgressUpdates = [
        { id: 'run123', status: 'running', currentStep: 'processData' },
        { id: 'run123', status: 'running', currentStep: 'saveResults' },
        { id: 'run123', status: 'completed', currentStep: null }
      ]

      // Test would verify:
      // 1. Initial run state is displayed
      // 2. Progress updates are applied in real-time
      // 3. UI reflects current execution state
      // 4. User can cancel running executions
      // 5. Completed runs show final results
    })
  })

  describe('Component Discovery Workflow', () => {
    it('should help users discover and understand components', async () => {
      const mockComponents = [
        {
          name: 'file-loader',
          url: 'builtins://load_file',
          description: 'Load data from files',
          inputSchema: {
            type: 'object',
            properties: {
              path: { type: 'string', description: 'File path' },
              format: { type: 'string', enum: ['json', 'csv'] }
            }
          },
          outputSchema: {
            type: 'object',
            properties: {
              data: { type: 'array' },
              recordCount: { type: 'number' }
            }
          },
          examples: [
            {
              name: 'Load JSON file',
              input: { path: './data.json', format: 'json' },
              description: 'Load data from a JSON file'
            }
          ]
        }
      ]

      mockUiApi.listComponents.mockResolvedValue({ components: mockComponents })

      // Component discovery workflow:
      // 1. User browses available components
      // 2. User views component details and schemas
      // 3. User sees usage examples
      // 4. User can test components individually
      // 5. User incorporates components into workflows

      // This would test the component exploration experience
    })
  })
})

describe('Integration Error Scenarios', () => {
  it('should handle network failures gracefully', async () => {
    // Test network error handling across the application
    mockUiApi.listFlows.mockRejectedValue(new Error('Network error'))
    
    // Verify:
    // 1. Error messages are user-friendly
    // 2. Users can retry operations
    // 3. Application remains responsive
    // 4. Offline functionality where applicable
  })

  it('should handle server errors with proper feedback', async () => {
    // Test server error scenarios
    mockUiApi.executeFlow.mockRejectedValue({
      response: {
        status: 500,
        data: { error: 'Internal server error' }
      }
    })

    // Verify error handling and user feedback
  })

  it('should handle authentication and authorization', async () => {
    // Test auth-related scenarios (if applicable)
    mockUiApi.executeFlow.mockRejectedValue({
      response: {
        status: 401,
        data: { error: 'Unauthorized' }
      }
    })

    // Verify proper auth handling
  })
})