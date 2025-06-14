import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '../api'
import type { 
  ExecutionDetails, 
  StepExecution, 
  Workflow
} from '../api'

// Query Keys
export const queryKeys = {
  health: ['health'],
  executions: ['executions'],
  execution: (id: string) => ['executions', id],
  executionSteps: (id: string) => ['executions', id, 'steps'],
  executionWorkflow: (id: string) => ['executions', id, 'workflow'],
  endpoints: ['endpoints'],
  endpoint: (name: string, label?: string) => ['endpoints', name, label],
  endpointWorkflow: (name: string, label?: string) => ['endpoints', name, 'workflow', label],
  components: ['components'],
  debugRunnableSteps: (executionId: string) => ['executions', executionId, 'debug', 'runnable'],
} as const

// Health
export const useHealth = () => {
  return useQuery({
    queryKey: queryKeys.health,
    queryFn: apiClient.health,
    refetchInterval: 30000, // Check health every 30 seconds
    retry: 1,
  })
}

// Executions
export const useExecutions = () => {
  return useQuery({
    queryKey: queryKeys.executions,
    queryFn: apiClient.listExecutions,
    refetchInterval: 5000, // Refresh every 5 seconds for active executions
    select: (data) => data.executions,
  })
}

export const useExecution = (executionId: string) => {
  return useQuery({
    queryKey: queryKeys.execution(executionId),
    queryFn: () => apiClient.getExecution(executionId),
    enabled: !!executionId,
    refetchInterval: (query) => {
      // More frequent updates for running executions
      const execution = query.state.data as ExecutionDetails | undefined
      return execution?.status === 'Running' ? 2000 : 10000
    },
  })
}

export const useExecutionSteps = (executionId: string) => {
  return useQuery({
    queryKey: queryKeys.executionSteps(executionId),
    queryFn: () => apiClient.getExecutionSteps(executionId),
    enabled: !!executionId,
    refetchInterval: (query) => {
      // More frequent updates during active execution
      const steps = query.state.data as { steps: StepExecution[] } | undefined
      const hasRunningSteps = steps?.steps.some(step => 
        !step.result || (!step.result.Success && !step.result.Failed && !step.result.Skipped)
      )
      return hasRunningSteps ? 2000 : 10000
    },
    select: (data) => data.steps,
  })
}

export const useExecutionWorkflow = (executionId: string) => {
  return useQuery({
    queryKey: queryKeys.executionWorkflow(executionId),
    queryFn: () => apiClient.getExecutionWorkflow(executionId),
    enabled: !!executionId,
    staleTime: 5 * 60 * 1000, // Workflows don't change, cache for 5 minutes
  })
}

// Endpoints
export const useEndpoints = () => {
  return useQuery({
    queryKey: queryKeys.endpoints,
    queryFn: apiClient.listEndpoints,
    select: (data) => data.endpoints,
  })
}

export const useEndpoint = (name: string, label?: string) => {
  return useQuery({
    queryKey: queryKeys.endpoint(name, label),
    queryFn: () => apiClient.getEndpoint(name, label),
    enabled: !!name,
  })
}

export const useEndpointWorkflow = (name: string, label?: string) => {
  return useQuery({
    queryKey: queryKeys.endpointWorkflow(name, label),
    queryFn: () => apiClient.getEndpointWorkflow(name, label),
    enabled: !!name,
    staleTime: 5 * 60 * 1000, // Workflows don't change often, cache for 5 minutes
  })
}

// Components
export const useComponents = (includeSchemas: boolean = true) => {
  return useQuery({
    queryKey: [...queryKeys.components, includeSchemas],
    queryFn: () => apiClient.listComponents(includeSchemas),
    staleTime: 5 * 60 * 1000, // Components don't change often, cache for 5 minutes
    select: (data) => data.components,
  })
}

// Debug Execution
export const useDebugRunnableSteps = (executionId: string) => {
  return useQuery({
    queryKey: queryKeys.debugRunnableSteps(executionId),
    queryFn: () => apiClient.getDebugRunnableSteps(executionId),
    enabled: !!executionId,
    refetchInterval: 1000,
    select: (data) => data.steps,
  })
}

// Mutations
export const useExecuteWorkflow = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: apiClient.execute,
    onSuccess: () => {
      // Invalidate executions list to show new execution
      queryClient.invalidateQueries({ queryKey: queryKeys.executions })
    },
  })
}

export const useCreateEndpoint = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ name, data, label }: { name: string; data: { workflow: Workflow }; label?: string }) => 
      apiClient.createEndpoint(name, data, label),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.endpoints })
    },
  })
}

export const useDeleteEndpoint = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ name, label }: { name: string; label?: string }) =>
      apiClient.deleteEndpoint(name, label),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.endpoints })
    },
  })
}

export const useExecuteEndpoint = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ name, data, label }: { name: string; data: { input: unknown; debug?: boolean }; label?: string }) => 
      apiClient.executeEndpoint(name, data, label),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.executions })
    },
  })
}

export const useExecuteDebugSteps = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ executionId, data }: { executionId: string; data: { step_ids: string[] } }) => 
      apiClient.executeDebugSteps(executionId, data),
    onSuccess: (_, { executionId }) => {
      // Refresh execution data
      queryClient.invalidateQueries({ queryKey: queryKeys.execution(executionId) })
      queryClient.invalidateQueries({ queryKey: queryKeys.executionSteps(executionId) })
      queryClient.invalidateQueries({ queryKey: queryKeys.debugRunnableSteps(executionId) })
    },
  })
}

export const useContinueDebugExecution = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: apiClient.continueDebugExecution,
    onSuccess: (_, executionId) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.execution(executionId) })
      queryClient.invalidateQueries({ queryKey: queryKeys.executionSteps(executionId) })
      queryClient.invalidateQueries({ queryKey: queryKeys.debugRunnableSteps(executionId) })
    },
  })
}

// Utility function to transform step execution data for the workflow visualizer
export const transformStepsForVisualizer = (steps: StepExecution[], workflow?: Workflow) => {
  if (!workflow?.steps) return []

  return workflow.steps.map((workflowStep, index) => {
    const stepExecution = steps.find(s => s.step_index === index)
    
    let status: 'completed' | 'running' | 'failed' | 'pending' = 'pending'
    let output: string | null = null
    let startTime: string | null = null
    let duration: string | null = null

    if (stepExecution?.result) {
      if (stepExecution.result.Success !== undefined) {
        status = 'completed'
        output = JSON.stringify(stepExecution.result.Success)
      } else if (stepExecution.result.Failed) {
        status = 'failed'
        output = JSON.stringify(stepExecution.result.Failed)
      } else if (stepExecution.result.Skipped !== undefined) {
        status = 'completed' // Skipped is considered completed
        output = 'Skipped'
      }
    } else if (stepExecution?.started_at && !stepExecution?.completed_at) {
      status = 'running'
    }

    if (stepExecution?.started_at) {
      startTime = new Date(stepExecution.started_at).toLocaleTimeString()
      
      if (stepExecution.completed_at) {
        const start = new Date(stepExecution.started_at)
        const end = new Date(stepExecution.completed_at)
        const diffMs = end.getTime() - start.getTime()
        const diffSecs = (diffMs / 1000).toFixed(1)
        duration = `${diffSecs}s`
      }
    }

    return {
      id: workflowStep.id,
      name: workflowStep.id,
      component: workflowStep.component,
      status,
      dependencies: workflowStep.depends_on || [],
      startTime,
      duration,
      output
    }
  })
}