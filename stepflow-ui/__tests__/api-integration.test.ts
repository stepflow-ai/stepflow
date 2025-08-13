import { getStepflowClient } from '@/lib/stepflow-client'

// Create mock API instances
const mockFlowApi = {
  storeFlow: jest.fn(),
  getFlow: jest.fn(),
}

const mockRunApi = {
  listRuns: jest.fn(),
  createRun: jest.fn(),
  getRun: jest.fn(),
  getRunSteps: jest.fn(),
}

const mockComponentApi = {
  listComponents: jest.fn(),
}

const mockHealthApi = {
  healthCheck: jest.fn(),
}

const mockDebugApi = {
  debugExecuteStep: jest.fn(),
}

// Mock the generated API client
jest.mock('../stepflow-api-client', () => ({
  Configuration: jest.fn(),
  FlowApi: jest.fn().mockImplementation(() => mockFlowApi),
  RunApi: jest.fn().mockImplementation(() => mockRunApi),
  ComponentApi: jest.fn().mockImplementation(() => mockComponentApi),
  HealthApi: jest.fn().mockImplementation(() => mockHealthApi),
  DebugApi: jest.fn().mockImplementation(() => mockDebugApi),
}))

describe('Stepflow Client API Integration', () => {
  let client: ReturnType<typeof getStepflowClient>

  beforeEach(() => {
    client = getStepflowClient()
  })

  afterEach(() => {
    jest.clearAllMocks()
  })

  describe('listRuns response handling', () => {
    it('should extract runs array from ListRunsResponse wrapper', async () => {
      const mockListRunsResponse = {
        runs: [
          { runId: 'run1', status: 'completed' },
          { runId: 'run2', status: 'running' }
        ]
      }

      mockRunApi.listRuns.mockResolvedValue({
        data: mockListRunsResponse
      })

      const result = await client.listRuns()

      expect(result).toEqual(mockListRunsResponse.runs)
      expect(Array.isArray(result)).toBe(true)
      expect(result).toHaveLength(2)
    })

    it('should handle empty runs response', async () => {
      mockRunApi.listRuns.mockResolvedValue({
        data: { runs: [] }
      })

      const result = await client.listRuns()

      expect(result).toEqual([])
      expect(Array.isArray(result)).toBe(true)
    })

    it('should handle malformed response gracefully', async () => {
      mockRunApi.listRuns.mockResolvedValue({
        data: {}
      })

      const result = await client.listRuns()

      expect(result).toEqual([])
      expect(Array.isArray(result)).toBe(true)
    })
  })

  describe('listComponents response handling', () => {
    it('should extract components array from ListComponentsResponse wrapper', async () => {
      const mockComponentsResponse = {
        components: [
          { name: 'eval', description: 'Evaluate expressions' },
          { name: 'openai', description: 'OpenAI API integration' }
        ]
      }

      mockComponentApi.listComponents.mockResolvedValue({
        data: mockComponentsResponse
      })

      const result = await client.listComponents()

      expect(result).toEqual(mockComponentsResponse.components)
      expect(Array.isArray(result)).toBe(true)
      expect(result).toHaveLength(2)
    })

    it('should handle empty components response', async () => {
      mockComponentApi.listComponents.mockResolvedValue({
        data: { components: [] }
      })

      const result = await client.listComponents()

      expect(result).toEqual([])
      expect(Array.isArray(result)).toBe(true)
    })
  })

  describe('getRunSteps response handling', () => {
    it('should extract steps array from ListStepRunsResponse wrapper', async () => {
      const mockStepsResponse = {
        steps: [
          { stepId: 'step1', status: 'completed' },
          { stepId: 'step2', status: 'running' }
        ]
      }

      mockRunApi.getRunSteps.mockResolvedValue({
        data: mockStepsResponse
      })

      const result = await client.getRunSteps('run123')

      expect(result).toEqual(mockStepsResponse.steps)
      expect(Array.isArray(result)).toBe(true)
      expect(result).toHaveLength(2)
    })

    it('should handle empty steps response', async () => {
      mockRunApi.getRunSteps.mockResolvedValue({
        data: { steps: [] }
      })

      const result = await client.getRunSteps('run123')

      expect(result).toEqual([])
      expect(Array.isArray(result)).toBe(true)
    })
  })

  describe('storeFlow response handling', () => {
    it('should return StoreFlowResponse directly', async () => {
      const mockStoreResponse = {
        flowId: 'abc123def456'
      }

      mockFlowApi.storeFlow.mockResolvedValue({
        data: mockStoreResponse
      })

      const result = await client.storeFlow({ name: 'test-flow', steps: [] })

      expect(result).toEqual(mockStoreResponse)
      expect(result.flowId).toBe('abc123def456')
    })
  })

  describe('createRun response handling', () => {
    it('should return CreateRunResponse directly', async () => {
      const mockRunResponse = {
        runId: 'run123',
        status: 'running',
        debug: false
      }

      mockRunApi.createRun.mockResolvedValue({
        data: mockRunResponse
      })

      const result = await client.createRun({
        flowId: 'abc123',
        input: { test: 'data' },
        debug: false
      })

      expect(result).toEqual(mockRunResponse)
      expect(result.runId).toBe('run123')
    })
  })

  describe('ad-hoc execution workflow', () => {
    it('should store workflow then execute by hash', async () => {
      const workflow = {
        name: 'test-workflow',
        steps: [{ id: 'step1', component: 'eval', inputs: { expr: '1 + 1' } }]
      }

      const mockStoreResponse = { flowId: 'abc123def456' }
      const mockRunResponse = { runId: 'run123', status: 'running', debug: false }

      mockFlowApi.storeFlow.mockResolvedValue({ data: mockStoreResponse })
      mockRunApi.createRun.mockResolvedValue({ data: mockRunResponse })

      // Store workflow
      const storeResult = await client.storeFlow(workflow)
      expect(storeResult.flowId).toBe('abc123def456')

      // Execute by hash
      const runResult = await client.createRun({
        flowId: storeResult.flowId,
        input: { test: 'input' },
        debug: false
      })
      expect(runResult.runId).toBe('run123')

      // Verify API calls
      expect(mockFlowApi.storeFlow).toHaveBeenCalledWith({ flow: workflow })
      expect(mockRunApi.createRun).toHaveBeenCalledWith({
        flowId: 'abc123def456',
        input: { test: 'input' },
        debug: false
      })
    })
  })
})