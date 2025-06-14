import axios from 'axios'

// Create axios instance with base configuration
export const api = axios.create({
  baseURL: process.env.STEPFLOW_API_URL || 'http://localhost:7837/api/v1',
  headers: {
    'Content-Type': 'application/json',
  },
  timeout: 30000,
})

// Request interceptor for logging
api.interceptors.request.use(
  (config) => {
    console.log(`API Request: ${config.method?.toUpperCase()} ${config.url}`)
    return config
  },
  (error) => {
    console.error('API Request Error:', error)
    return Promise.reject(error)
  }
)

// Response interceptor for error handling
api.interceptors.response.use(
  (response) => {
    console.log(`API Response: ${response.status} ${response.config.url}`)
    return response
  },
  (error) => {
    console.error('API Response Error:', error.response?.data || error.message)
    return Promise.reject(error)
  }
)

// API Functions
export const apiClient = {
  // Health check
  health: async (): Promise<{ status: string; timestamp: string }> => {
    const response = await api.get('/health')
    return response.data
  },

  // Executions
  getExecution: async (executionId: string): Promise<ExecutionDetails> => {
    const response = await api.get(`/executions/${executionId}`)
    return response.data
  },

  listExecutions: async (): Promise<ListExecutionsResponse> => {
    const response = await api.get('/executions')
    return response.data
  },

  getExecutionSteps: async (executionId: string): Promise<{ steps: StepExecution[] }> => {
    const response = await api.get(`/executions/${executionId}/steps`)
    return response.data
  },

  getExecutionWorkflow: async (executionId: string): Promise<{ workflow: Workflow; workflow_hash: string }> => {
    const response = await api.get(`/executions/${executionId}/workflow`)
    return response.data
  },

  // Endpoints
  listEndpoints: async (): Promise<ListEndpointsResponse> => {
    const response = await api.get('/endpoints')
    return response.data
  },

  getEndpoint: async (name: string, label?: string): Promise<EndpointDetails> => {
    const url = label ? `/endpoints/${name}?label=${label}` : `/endpoints/${name}`
    const response = await api.get(url)
    return response.data
  },

  createEndpoint: async (name: string, data: { workflow: Workflow }, label?: string): Promise<EndpointDetails> => {
    const url = label ? `/endpoints/${name}?label=${label}` : `/endpoints/${name}`
    const response = await api.put(url, data)
    return response.data
  },

  deleteEndpoint: async (name: string, label?: string): Promise<{ message: string }> => {
    const url = label ? `/endpoints/${name}?label=${label}` : `/endpoints/${name}`
    const response = await api.delete(url)
    return response.data
  },

  executeEndpoint: async (name: string, data: { input: unknown; debug?: boolean }, label?: string): Promise<ExecuteResponse> => {
    const url = label ? `/endpoints/${name}/execute?label=${label}` : `/endpoints/${name}/execute`
    const response = await api.post(url, data)
    return response.data
  },

  getEndpointWorkflow: async (name: string, label?: string): Promise<{ workflow: Workflow; workflow_hash: string }> => {
    const url = label ? `/endpoints/${name}/workflow?label=${label}` : `/endpoints/${name}/workflow`
    const response = await api.get(url)
    return response.data
  },

  // Ad-hoc execution
  execute: async (data: { workflow: Workflow; input: unknown; debug?: boolean }): Promise<ExecuteResponse> => {
    const response = await api.post('/executions', data)
    return response.data
  },

  // Components
  listComponents: async (includeSchemas: boolean = true): Promise<ListComponentsResponse> => {
    const response = await api.get('/components', {
      params: {
        include_schemas: includeSchemas
      }
    })
    return response.data
  },

  // Debug Execution (for debug mode executions)
  getDebugRunnableSteps: async (executionId: string): Promise<{ steps: string[] }> => {
    const response = await api.get(`/executions/${executionId}/debug/runnable`)
    return response.data
  },

  executeDebugSteps: async (executionId: string, data: { step_ids: string[] }): Promise<{ steps: unknown[] }> => {
    const response = await api.post(`/executions/${executionId}/debug/step`, data)
    return response.data
  },

  continueDebugExecution: async (executionId: string): Promise<ExecuteResponse> => {
    const response = await api.post(`/executions/${executionId}/debug/continue`)
    return response.data
  },

  // Workflow management
  storeWorkflow: async (workflow: Workflow): Promise<{ workflow_hash: string }> => {
    const response = await api.post('/workflows', { workflow })
    return response.data
  },

  getWorkflow: async (workflowHash: string): Promise<{ workflow: Workflow; workflow_hash: string }> => {
    const response = await api.get(`/workflows/${workflowHash}`)
    return response.data
  },

  listWorkflows: async (): Promise<{ workflows: Array<{ workflow_hash: string }> }> => {
    const response = await api.get('/workflows')
    return response.data
  },

  getWorkflowDependencies: async (workflowHash: string): Promise<WorkflowDependenciesResponse> => {
    const response = await api.get(`/workflows/${workflowHash}/dependencies`)
    return response.data
  },

  getEndpointDependencies: async (name: string, label?: string): Promise<WorkflowDependenciesResponse> => {
    const url = label ? `/endpoints/${name}/dependencies?label=${label}` : `/endpoints/${name}/dependencies`
    const response = await api.get(url)
    return response.data
  },
}

// Type definitions
export interface ExecutionDetails {
  execution_id: string
  endpoint_name?: string
  workflow_hash?: string
  status: 'Pending' | 'Running' | 'Completed' | 'Failed'
  created_at: string
  completed_at?: string
  debug_mode: boolean
  result?: unknown
}

export interface StepExecution {
  step_index: number
  step_id?: string
  component?: string
  state: 'blocked' | 'runnable' | 'running' | 'completed' | 'failed' | 'skipped'
  result?: {
    outcome: 'success' | 'failed' | 'skipped'
    result?: unknown
    error?: {
      code: number
      message: string
    }
  }
}

export interface Workflow {
  name?: string
  description?: string
  input_schema?: Record<string, unknown>
  output?: Record<string, unknown>
  steps: WorkflowStep[]
}

export interface WorkflowStep {
  id: string
  component: string
  input?: Record<string, unknown>
  depends_on?: string[]
  config?: Record<string, unknown>
}

export interface ExecuteResponse {
  execution_id: string
  result?: unknown
  status: string
}

export interface ListExecutionsResponse {
  executions: ExecutionSummary[]
}

export interface ExecutionSummary {
  execution_id: string
  endpoint_name?: string
  workflow_hash?: string
  status: 'Pending' | 'Running' | 'Completed' | 'Failed'
  created_at: string
  completed_at?: string
  debug_mode: boolean
}

export interface ListEndpointsResponse {
  endpoints: EndpointSummary[]
}

export interface EndpointSummary {
  name: string
  label?: string
  workflow_hash: string
  created_at: string
  updated_at: string
}

export interface EndpointDetails {
  name: string
  label?: string
  workflow_hash: string
  created_at: string
  updated_at: string
}

export interface ListComponentsResponse {
  components: ComponentInfo[]
}

export interface ComponentInfo {
  name: string
  plugin_name: string
  description?: string
  input_schema?: unknown
  output_schema?: unknown
}

export interface WorkflowDependenciesResponse {
  workflow_hash: string
  dependencies: StepDependency[]
}

export interface StepDependency {
  step_index: number
  depends_on_step_index: number
  src_path?: string
  dst_field: DestinationField
  skip_action: SkipAction
}

export interface DestinationField {
  skip_if?: boolean
  input?: boolean
  input_field?: string
  output?: boolean
}

export interface SkipAction {
  action: 'skip' | 'use_default'
  default_value?: unknown
}

export default apiClient