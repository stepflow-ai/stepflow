import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { apiClient } from '../api'
import type { 
  ExecutionDetails, 
  StepExecution, 
  Workflow,
  WorkflowDependenciesResponse
} from '../api'

// Query Keys
export const queryKeys = {
  health: ['health'],
  executions: ['executions'],
  execution: (id: string) => ['executions', id],
  executionSteps: (id: string) => ['executions', id, 'steps'],
  executionWorkflow: (id: string) => ['executions', id, 'workflow'],
  workflowNames: ['workflows', 'names'],
  workflowsByName: (name: string) => ['workflows', 'by-name', name],
  latestWorkflowByName: (name: string) => ['workflows', 'by-name', name, 'latest'],
  workflowByLabel: (name: string, label: string) => ['workflows', 'by-name', name, 'labels', label],
  labelsForName: (name: string) => ['workflows', 'by-name', name, 'labels'],
  workflowDependencies: (workflowHash: string) => ['workflows', workflowHash, 'dependencies'],
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
        step.state === 'running' || step.state === 'runnable'
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


export const useWorkflowDependencies = (workflowHash: string) => {
  return useQuery({
    queryKey: queryKeys.workflowDependencies(workflowHash),
    queryFn: () => apiClient.getWorkflowDependencies(workflowHash),
    enabled: !!workflowHash,
    staleTime: 5 * 60 * 1000, // Dependencies don't change often, cache for 5 minutes
  })
}

// Workflows
export const useWorkflowNames = () => {
  return useQuery({
    queryKey: queryKeys.workflowNames,
    queryFn: apiClient.listWorkflowNames,
    select: (data) => data.names,
  })
}

export const useWorkflowsByName = (name: string) => {
  return useQuery({
    queryKey: queryKeys.workflowsByName(name),
    queryFn: () => apiClient.getWorkflowsByName(name),
    enabled: !!name,
    staleTime: 5 * 60 * 1000, // Workflow versions don't change often
  })
}

export const useLatestWorkflowByName = (name: string) => {
  return useQuery({
    queryKey: queryKeys.latestWorkflowByName(name),
    queryFn: () => apiClient.getLatestWorkflowByName(name),
    enabled: !!name,
    staleTime: 5 * 60 * 1000, // Workflows don't change often
  })
}

export const useWorkflowByLabel = (name: string, label: string) => {
  return useQuery({
    queryKey: queryKeys.workflowByLabel(name, label),
    queryFn: () => apiClient.getWorkflowByLabel(name, label),
    enabled: !!name && !!label,
    staleTime: 5 * 60 * 1000, // Labeled workflows don't change often
  })
}

export const useLabelsForName = (name: string) => {
  return useQuery({
    queryKey: queryKeys.labelsForName(name),
    queryFn: () => apiClient.listLabelsForName(name),
    enabled: !!name,
    select: (data) => data.labels,
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

export const useExecuteWorkflowByName = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ name, data }: { name: string; data: { input: unknown; debug?: boolean } }) => 
      apiClient.executeWorkflowByName(name, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.executions })
    },
  })
}

export const useExecuteWorkflowByLabel = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ name, label, data }: { name: string; label: string; data: { input: unknown; debug?: boolean } }) => 
      apiClient.executeWorkflowByLabel(name, label, data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.executions })
    },
  })
}

export const useCreateOrUpdateLabel = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ name, label, workflowHash }: { name: string; label: string; workflowHash: string }) => 
      apiClient.createOrUpdateLabel(name, label, workflowHash),
    onSuccess: (_, { name }) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.labelsForName(name) })
      queryClient.invalidateQueries({ queryKey: queryKeys.workflowNames })
    },
  })
}

export const useDeleteLabel = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationFn: ({ name, label }: { name: string; label: string }) =>
      apiClient.deleteLabel(name, label),
    onSuccess: (_, { name }) => {
      queryClient.invalidateQueries({ queryKey: queryKeys.labelsForName(name) })
      queryClient.invalidateQueries({ queryKey: queryKeys.workflowNames })
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

    if (stepExecution?.state) {
      // Map the new state field to status
      switch (stepExecution.state) {
        case 'completed':
          status = 'completed'
          break
        case 'running':
          status = 'running'
          break
        case 'failed':
          status = 'failed'
          break
        case 'skipped':
          status = 'completed' // Skipped is considered completed for visualization
          break
        case 'blocked':
        case 'runnable':
        default:
          status = 'pending'
          break
      }
    }

    // Handle result output
    if (stepExecution?.result) {
      switch (stepExecution.result.outcome) {
        case 'success':
          output = stepExecution.result.result ? JSON.stringify(stepExecution.result.result) : 'Success'
          break
        case 'failed':
          output = stepExecution.result.error ? 
            `Error ${stepExecution.result.error.code}: ${stepExecution.result.error.message}` : 
            'Failed'
          break
        case 'skipped':
          output = 'Skipped'
          break
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