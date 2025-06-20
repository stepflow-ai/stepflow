// Core StepFlow server client
// Uses the generated OpenAPI client for type safety

import { Configuration, FlowApi, RunApi, ComponentApi, HealthApi, DebugApi } from '../stepflow-api-client'

interface StepFlowConfig {
  baseUrl: string
  timeout?: number
}

export class StepFlowClient {
  private flowApi: FlowApi
  private runApi: RunApi
  private componentApi: ComponentApi
  private healthApi: HealthApi
  private debugApi: DebugApi

  constructor(config?: Partial<StepFlowConfig>) {
    const baseUrl = config?.baseUrl || process.env.STEPFLOW_SERVER_URL || 'http://localhost:7837/api/v1'

    const configuration = new Configuration({
      basePath: baseUrl,
    })

    this.flowApi = new FlowApi(configuration)
    this.runApi = new RunApi(configuration)
    this.componentApi = new ComponentApi(configuration)
    this.healthApi = new HealthApi(configuration)
    this.debugApi = new DebugApi(configuration)
  }

  // Flow management
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  async storeFlow(flow: any) {
    const response = await this.flowApi.storeFlow({ flow })
    return response.data
  }

  async getFlow(flowHash: string) {
    const response = await this.flowApi.getFlow(flowHash)
    return response.data
  }

  async deleteFlow(flowHash: string) {
    const response = await this.flowApi.deleteFlow(flowHash)
    return response.data
  }

  // Note: getFlowDependencies was removed - dependencies are now included in getFlow response
  async getFlowDependencies(flowHash: string) {
    // Dependencies are now included in the FlowResponse.analysis field from getFlow
    const flowResponse = await this.getFlow(flowHash)
    return flowResponse.analysis
  }

  // Run management
  async createRun(request: { flowHash: string; input: unknown; debug?: boolean }) {
    const response = await this.runApi.createRun({
      flowHash: request.flowHash,
      input: request.input,
      debug: request.debug,
    })
    return response.data
  }

  async getRun(runId: string) {
    const response = await this.runApi.getRun(runId)
    return response.data
  }

  async getRunFlow(runId: string) {
    const response = await this.runApi.getRunFlow(runId)
    return response.data
  }

  async listRuns() {
    const response = await this.runApi.listRuns()
    return response.data.runs || []
  }

  async getRunSteps(runId: string) {
    const response = await this.runApi.getRunSteps(runId)
    return response.data.steps || []
  }

  async cancelRun(runId: string) {
    const response = await this.runApi.cancelRun(runId)
    return response.data
  }

  async deleteRun(runId: string) {
    const response = await this.runApi.deleteRun(runId)
    return response.data
  }

  // Component management
  async listComponents(includeSchemas = false) {
    const response = await this.componentApi.listComponents(includeSchemas)
    return response.data.components || []
  }

  // Health check
  async healthCheck() {
    const response = await this.healthApi.healthCheck()
    return response.data
  }

  // Debug operations
  async debugExecuteStep(runId: string, stepIds: string[]) {
    const response = await this.debugApi.debugExecuteStep(runId, { stepIds: stepIds })
    return response.data
  }

  async debugContinue(runId: string) {
    const response = await this.debugApi.debugContinue(runId)
    return response.data
  }

  async debugGetRunnable(runId: string) {
    const response = await this.debugApi.debugGetRunnable(runId)
    return response.data
  }
}

// Singleton instance
let clientInstance: StepFlowClient | null = null

export function getStepFlowClient(): StepFlowClient {
  if (!clientInstance) {
    clientInstance = new StepFlowClient()
  }
  return clientInstance
}