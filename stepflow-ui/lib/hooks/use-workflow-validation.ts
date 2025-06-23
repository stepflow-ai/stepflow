import { useMutation } from '@tanstack/react-query'
import { StoreFlowRequest, FlowApi } from '@/stepflow-api-client'
import { StoreFlowResponse } from '@/lib/api-types'

export interface WorkflowValidationRequest {
  workflow: Record<string, unknown>
  checkOnly?: boolean
}

export const useWorkflowValidation = () => {
  const flowApi = new FlowApi()

  return useMutation({
    mutationKey: ['validateWorkflow'],
    mutationFn: async (request: WorkflowValidationRequest): Promise<StoreFlowResponse> => {
      try {
        const storeRequest: StoreFlowRequest = {
          flow: request.workflow
        }

        const response = await flowApi.storeFlow(storeRequest)
        return response.data
      } catch (error: unknown) {
        // Parse error response and create a diagnostic response
        throw error
      }
    },
  })
}

// Enhanced validation hook that can handle both validation-only and store operations
export const useStoreFlowWithValidation = () => {
  const flowApi = new FlowApi()

  return useMutation({
    mutationKey: ['storeFlowWithValidation'],
    mutationFn: async (request: {
      flow: Record<string, unknown>
      name?: string
      description?: string
      checkOnly?: boolean
    }): Promise<StoreFlowResponse> => {
      try {
        if (request.checkOnly) {
          // For validation-only mode, we could create a temporary flow just for analysis
          // For now, we'll store it and ignore the hash in the UI
        }

        const storeRequest: StoreFlowRequest = {
          flow: request.flow
        }

        const response = await flowApi.storeFlow(storeRequest)
        return response.data
      } catch (error: unknown) {
        // Parse error response for diagnostics
        throw error
      }
    },
  })
}