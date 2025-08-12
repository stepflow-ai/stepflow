import { useMutation } from '@tanstack/react-query'
import { StoreFlowResponse } from '@/lib/api-types'

export interface WorkflowValidationRequest {
  workflow: Record<string, unknown>
  checkOnly?: boolean
}

export const useWorkflowValidation = () => {
  return useMutation({
    mutationKey: ['validateWorkflow'],
    mutationFn: async (request: WorkflowValidationRequest): Promise<StoreFlowResponse> => {
      const response = await fetch('/api/validate', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          workflow: request.workflow,
        }),
      })
      
      if (!response.ok) {
        const error = await response.json()
        throw new Error(error.message || error.error || `HTTP ${response.status}`)
      }
      
      const result = await response.json()
      return result as StoreFlowResponse
    },
  })
}

// Enhanced validation hook that can handle both validation-only and store operations
export const useStoreFlowWithValidation = () => {
  return useMutation({
    mutationKey: ['storeFlowWithValidation'],
    mutationFn: async (request: {
      flow: Record<string, unknown>
      name?: string
      description?: string
      checkOnly?: boolean
    }): Promise<StoreFlowResponse> => {
      if (request.checkOnly) {
        // For validation-only, use the validation API
        const response = await fetch('/api/validate', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({
            workflow: request.flow,
          }),
        })
        
        if (!response.ok) {
          const error = await response.json()
          throw new Error(error.message || error.error || `HTTP ${response.status}`)
        }
        
        const result = await response.json()
        return result as StoreFlowResponse
      } else {
        // For actual storage, use the flows API
        if (!request.name) {
          throw new Error('Name is required for storing flows')
        }
        
        const response = await fetch('/api/flows', {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({
            name: request.name,
            description: request.description,
            flow: request.flow,
          }),
        })
        
        if (!response.ok) {
          let errorMessage = `HTTP ${response.status}`
          try {
            const error = await response.json()
            errorMessage = error.message || error.error || errorMessage
          } catch (parseError) {
            try {
              const text = await response.text()
              errorMessage = `${errorMessage}: ${text}`
            } catch (textError) {
              errorMessage = `${errorMessage} (Failed to read response)`
            }
          }
          throw new Error(errorMessage)
        }
        
        const result = await response.json()
        return result as StoreFlowResponse
      }
    },
  })
}