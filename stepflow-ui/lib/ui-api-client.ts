// Frontend API client for UI server
// Uses shared types for type safety

import {
  type StoreWorkflowRequest,
  type WorkflowSummary,
  type WorkflowDetail,
  type ExecuteWorkflowRequest,
  type ExecuteAdHocWorkflowRequest,
  type ExecuteWorkflowResponse,
  type CreateLabelRequest,
  type LabelResponse,
  type ListWorkflowsResponse,
  type ListLabelsResponse,
  type RunDetailsResponse,
  type RunStepsResponse,
  type RunWorkflowResponse,
  type ListComponentsResponse,
  type ErrorResponse,
} from './api-types'

class UIApiClient {
  private baseUrl: string

  constructor(baseUrl = '/api') {
    this.baseUrl = baseUrl
  }

  private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
    const url = `${this.baseUrl}${path}`

    const response = await fetch(url, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...options.headers,
      },
    })

    const data = await response.json()

    if (!response.ok) {
      const error = data as ErrorResponse
      throw new Error(error.message || error.error || `HTTP ${response.status}`)
    }

    return data
  }

  // ============================================================================
  // Flow Management
  // ============================================================================

  async listFlows(): Promise<ListWorkflowsResponse> {
    return this.request<ListWorkflowsResponse>('/flows')
  }

  async getFlow(name: string): Promise<WorkflowDetail> {
    return this.request<WorkflowDetail>(`/flows/${encodeURIComponent(name)}`)
  }

  async storeFlow(request: StoreWorkflowRequest): Promise<WorkflowSummary> {
    return this.request<WorkflowSummary>('/flows', {
      method: 'POST',
      body: JSON.stringify(request),
    })
  }

  async deleteFlow(name: string): Promise<void> {
    await this.request<void>(`/flows/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    })
  }


  // ============================================================================
  // Flow Execution
  // ============================================================================

  async executeFlow(name: string, request: ExecuteWorkflowRequest): Promise<ExecuteWorkflowResponse> {
    return this.request<ExecuteWorkflowResponse>(`/flows/${encodeURIComponent(name)}/execute`, {
      method: 'POST',
      body: JSON.stringify(request),
    })
  }

  async executeFlowByLabel(
    name: string,
    label: string,
    request: ExecuteWorkflowRequest
  ): Promise<ExecuteWorkflowResponse> {
    return this.request<ExecuteWorkflowResponse>(
      `/flows/${encodeURIComponent(name)}/labels/${encodeURIComponent(label)}/execute`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      }
    )
  }

  async executeAdHocWorkflow(request: ExecuteAdHocWorkflowRequest): Promise<ExecuteWorkflowResponse> {
    return this.request<ExecuteWorkflowResponse>('/runs', {
      method: 'POST',
      body: JSON.stringify(request),
    })
  }

  // ============================================================================
  // Flow Label Management
  // ============================================================================

  async listFlowLabels(name: string): Promise<ListLabelsResponse> {
    return this.request<ListLabelsResponse>(`/flows/${encodeURIComponent(name)}/labels`)
  }

  async createFlowLabel(name: string, label: string, request: CreateLabelRequest): Promise<LabelResponse> {
    return this.request<LabelResponse>(
      `/flows/${encodeURIComponent(name)}/labels/${encodeURIComponent(label)}`,
      {
        method: 'POST',
        body: JSON.stringify(request),
      }
    )
  }

  async deleteFlowLabel(name: string, label: string): Promise<void> {
    await this.request<void>(
      `/flows/${encodeURIComponent(name)}/labels/${encodeURIComponent(label)}`,
      {
        method: 'DELETE',
      }
    )
  }


  // ============================================================================
  // Run Management (Proxy to Core Server)
  // ============================================================================

  async getRun(runId: string): Promise<RunDetailsResponse> {
    return this.request<RunDetailsResponse>(`/runs/${runId}`)
  }

  async getRunSteps(runId: string): Promise<RunStepsResponse> {
    return this.request<RunStepsResponse>(`/runs/${runId}/steps`)
  }

  async getRunWorkflow(runId: string): Promise<RunWorkflowResponse> {
    return this.request<RunWorkflowResponse>(`/runs/${runId}/workflow`)
  }

  async listRuns(): Promise<RunDetailsResponse[]> {
    return this.request<RunDetailsResponse[]>('/runs')
  }

  async cancelRun(runId: string): Promise<void> {
    await this.request<void>(`/runs/${runId}/cancel`, {
      method: 'POST',
    })
  }

  async deleteRun(runId: string): Promise<void> {
    await this.request<void>(`/runs/${runId}`, {
      method: 'DELETE',
    })
  }

  // ============================================================================
  // Components (Proxy to Core Server)
  // ============================================================================

  async listComponents(includeSchemas = true): Promise<ListComponentsResponse> {
    const params = includeSchemas ? '?include_schemas=true' : ''
    return this.request<ListComponentsResponse>(`/components${params}`)
  }

  // ============================================================================
  // Health Check
  // ============================================================================

  async healthCheck(): Promise<{ status: string; timestamp: string; version?: string }> {
    return this.request<{ status: string; timestamp: string; version?: string }>('/health')
  }
}

// Singleton instance
let apiClientInstance: UIApiClient | null = null

export function getUIApiClient(): UIApiClient {
  if (!apiClientInstance) {
    apiClientInstance = new UIApiClient()
  }
  return apiClientInstance
}

// Named exports for convenience
export const uiApi = getUIApiClient()

// Re-export types for convenience
export type {
  StoreWorkflowRequest,
  WorkflowSummary,
  WorkflowDetail,
  ExecuteWorkflowRequest,
  ExecuteWorkflowResponse,
  CreateLabelRequest,
  LabelResponse,
  ListWorkflowsResponse,
  ListLabelsResponse,
  RunDetailsResponse,
  RunStepsResponse,
  RunWorkflowResponse,
  ListComponentsResponse,
} from './api-types'