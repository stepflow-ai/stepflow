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

  getExecutionWorkflow: async (executionId: string): Promise<{ workflow: Workflow; workflow_hash: string; all_examples: ExampleInput[] }> => {
    const response = await api.get(`/executions/${executionId}/workflow`)
    return response.data
  },

  // Named Workflows
  listWorkflowNames: async (): Promise<ListWorkflowNamesResponse> => {
    const response = await api.get('/workflows/names')
    return response.data
  },

  getWorkflowsByName: async (name: string): Promise<WorkflowsByNameResponse> => {
    const response = await api.get(`/workflows/by-name/${name}`)
    return response.data
  },

  getLatestWorkflowByName: async (name: string): Promise<{ workflow: Workflow; workflow_hash: string; all_examples: ExampleInput[] }> => {
    const response = await api.get(`/workflows/by-name/${name}/latest`)
    return response.data
  },

  executeWorkflowByName: async (name: string, data: { input: unknown; debug?: boolean }): Promise<ExecuteResponse> => {
    const response = await api.post(`/workflows/by-name/${name}/execute`, data)
    return response.data
  },

  // Workflow Labels
  listLabelsForName: async (name: string): Promise<ListLabelsResponse> => {
    const response = await api.get(`/workflows/by-name/${name}/labels`)
    return response.data
  },

  createOrUpdateLabel: async (name: string, label: string, workflowHash: string): Promise<WorkflowLabelResponse> => {
    const response = await api.put(`/workflows/by-name/${name}/labels/${label}`, { workflow_hash: workflowHash })
    return response.data
  },

  getWorkflowByLabel: async (name: string, label: string): Promise<{ workflow: Workflow; workflow_hash: string; all_examples: ExampleInput[] }> => {
    const response = await api.get(`/workflows/by-name/${name}/labels/${label}`)
    return response.data
  },

  executeWorkflowByLabel: async (name: string, label: string, data: { input: unknown; debug?: boolean }): Promise<ExecuteResponse> => {
    const response = await api.post(`/workflows/by-name/${name}/labels/${label}/execute`, data)
    return response.data
  },

  deleteLabel: async (name: string, label: string): Promise<{ message: string }> => {
    const response = await api.delete(`/workflows/by-name/${name}/labels/${label}`)
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

  getWorkflow: async (workflowHash: string): Promise<{ workflow: Workflow; workflow_hash: string; all_examples: ExampleInput[] }> => {
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

  getWorkflowDependenciesByLabel: async (name: string, label: string): Promise<WorkflowDependenciesResponse> => {
    // First get the workflow by label
    const workflowResponse = await api.get(`/workflows/by-name/${name}/labels/${label}`)
    const workflowHash = workflowResponse.data.workflow_hash
    const response = await api.get(`/workflows/${workflowHash}/dependencies`)
    return response.data
  },

  getWorkflowDependenciesByName: async (name: string): Promise<WorkflowDependenciesResponse> => {
    // First get the latest workflow by name
    const workflowResponse = await api.get(`/workflows/by-name/${name}/latest`)
    const workflowHash = workflowResponse.data.workflow_hash
    const response = await api.get(`/workflows/${workflowHash}/dependencies`)
    return response.data
  },

}

// Type definitions
export interface ExecutionDetails {
  execution_id: string
  workflow_name?: string
  workflow_label?: string
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
  examples?: ExampleInput[]
}

export interface ExampleInput {
  name: string
  description?: string
  input: Record<string, unknown>
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
  workflow_name?: string
  workflow_label?: string
  workflow_hash?: string
  status: 'Pending' | 'Running' | 'Completed' | 'Failed'
  created_at: string
  completed_at?: string
  debug_mode: boolean
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

// New Workflow API Types
export interface ListWorkflowNamesResponse {
  names: string[]
}

export interface WorkflowsByNameResponse {
  name: string
  workflows: WorkflowVersionInfo[]
}

export interface WorkflowVersionInfo {
  workflow_hash: string
  created_at: string
}

export interface ListLabelsResponse {
  labels: WorkflowLabelResponse[]
}

export interface WorkflowLabelResponse {
  name: string
  label: string
  workflow_hash: string
  created_at: string
  updated_at: string
}

export default apiClient