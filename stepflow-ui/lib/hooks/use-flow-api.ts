import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { uiApi } from '@/lib/ui-api-client'
import type {
  StoreWorkflowRequest,
  RunDetailsResponse,
} from '@/lib/ui-api-client'

// ============================================================================
// Flow Management Hooks
// ============================================================================

export const useFlows = () => {
  return useQuery({
    queryKey: ['flows'],
    queryFn: () => uiApi.listFlows(),
    select: (data) => data.workflows,
    staleTime: 5 * 60 * 1000, // 5 minutes
  })
}

export const useFlow = (name: string) => {
  return useQuery({
    queryKey: ['flow', name],
    queryFn: () => uiApi.getFlow(name),
    enabled: !!name,
    staleTime: 5 * 60 * 1000, // 5 minutes
  })
}

export const useStoreFlow = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: ['storeFlow'],
    mutationFn: (request: StoreWorkflowRequest) => uiApi.storeFlow(request),
    onSuccess: (data) => {
      // Invalidate flows list
      queryClient.invalidateQueries({ queryKey: ['flows'] })
      // Update specific flow cache
      queryClient.setQueryData(['flow', data.name], data)
    },
  })
}

export const useDeleteFlow = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: ['deleteFlow'],
    mutationFn: (name: string) => uiApi.deleteFlow(name),
    onSuccess: (_, name) => {
      // Remove from cache and invalidate list
      queryClient.removeQueries({ queryKey: ['flow', name] })
      queryClient.invalidateQueries({ queryKey: ['flows'] })
    },
  })
}

// ============================================================================
// Flow Execution Hooks
// ============================================================================

export const useExecuteFlow = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: ['executeFlow'],
    mutationFn: ({ name, input, debug = false }: { name: string; input: Record<string, unknown>; debug?: boolean }) =>
      uiApi.executeFlow(name, { input, debug }),
    onSuccess: (data, variables) => {
      // Invalidate flow to refresh execution history
      queryClient.invalidateQueries({ queryKey: ['flow', variables.name] })
      // Cache the run details
      queryClient.setQueryData(['run', data.runId], data)
    },
  })
}

export const useExecuteFlowByLabel = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: ['executeFlowByLabel'],
    mutationFn: ({
      name,
      label,
      input,
      debug = false
    }: {
      name: string;
      label: string;
      input: Record<string, unknown>;
      debug?: boolean
    }) =>
      uiApi.executeFlowByLabel(name, label, { input, debug }),
    onSuccess: (data, variables) => {
      // Invalidate flow to refresh execution history
      queryClient.invalidateQueries({ queryKey: ['flow', variables.name] })
      // Cache the run details
      queryClient.setQueryData(['run', data.runId], data)
    },
  })
}

export const useExecuteAdHocWorkflow = () => {
  const queryClient = useQueryClient()

  return useMutation({
    mutationKey: ['executeAdHocWorkflow'],
    mutationFn: ({
      workflow,
      input,
      debug = false
    }: {
      workflow: Record<string, unknown>;
      input: Record<string, unknown>;
      debug?: boolean
    }) =>
      uiApi.executeAdHocWorkflow({ workflow, input, debug }),
    onSuccess: (data) => {
      // Invalidate runs list to show new execution
      queryClient.invalidateQueries({ queryKey: ['runs'] })
      // Cache the run details
      queryClient.setQueryData(['run', data.runId], data)
    },
  })
}

// ============================================================================
// Flow Label Management Hooks
// ============================================================================

export const useFlowLabels = (name: string) => {
  return useQuery({
    queryKey: ['flow-labels', name],
    queryFn: () => uiApi.listFlowLabels(name),
    enabled: !!name,
    select: (data) => data.labels,
    staleTime: 5 * 60 * 1000, // 5 minutes
  })
}

export const useCreateFlowLabel = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationKey: ['createFlowLabel'],
    mutationFn: ({ name, label, flowId }: { name: string; label: string; flowId: string }) =>
      uiApi.createFlowLabel(name, label, { flowId }),
    onSuccess: (_, variables) => {
      // Invalidate related queries
      queryClient.invalidateQueries({ queryKey: ['flow-labels', variables.name] })
      queryClient.invalidateQueries({ queryKey: ['flow', variables.name] })
    },
  })
}

export const useDeleteFlowLabel = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationKey: ['deleteFlowLabel'],
    mutationFn: ({ name, label }: { name: string; label: string }) =>
      uiApi.deleteFlowLabel(name, label),
    onSuccess: (_, variables) => {
      // Invalidate related queries
      queryClient.invalidateQueries({ queryKey: ['flow-labels', variables.name] })
      queryClient.invalidateQueries({ queryKey: ['flow', variables.name] })
    },
  })
}

// ============================================================================
// Run Management Hooks (Proxy to Core Server)
// ============================================================================

export const useRun = (runId: string) => {
  return useQuery({
    queryKey: ['run', runId],
    queryFn: () => uiApi.getRun(runId),
    enabled: !!runId,
    refetchInterval: (query) => {
      const run = query.state.data as RunDetailsResponse | undefined
      // Refresh running/paused runs more frequently
      return run?.status === 'running' || run?.status === 'paused' ? 2000 : 10000
    },
  })
}

export const useRunSteps = (runId: string) => {
  return useQuery({
    queryKey: ['run-steps', runId],
    queryFn: () => uiApi.getRunSteps(runId),
    enabled: !!runId,
    select: (data) => data.steps,
    refetchInterval: (query) => {
      const run = query.state.data as RunDetailsResponse | undefined
      // Refresh running/paused runs more frequently
      return run?.status === 'running' || run?.status === 'paused' ? 2000 : 10000
    },
  })
}

export const useRunWorkflow = (runId: string) => {
  return useQuery({
    queryKey: ['run-workflow', runId],
    queryFn: () => uiApi.getRunWorkflow(runId),
    enabled: !!runId,
    staleTime: 10 * 60 * 1000, // 10 minutes - workflow info is relatively static
  })
}

export const useRuns = () => {
  return useQuery({
    queryKey: ['runs'],
    queryFn: () => uiApi.listRuns(),
    staleTime: 30 * 1000, // 30 seconds - execution list changes frequently
    refetchInterval: 10000, // Refresh every 10 seconds
  })
}

export const useCancelRun = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationKey: ['cancelRun'],
    mutationFn: (runId: string) => uiApi.cancelRun(runId),
    onSuccess: (_, runId) => {
      // Invalidate run details to refresh status
      queryClient.invalidateQueries({ queryKey: ['run', runId] })
    },
  })
}

export const useDeleteRun = () => {
  const queryClient = useQueryClient()
  
  return useMutation({
    mutationKey: ['deleteRun'],
    mutationFn: (runId: string) => uiApi.deleteRun(runId),
    onSuccess: (_, runId) => {
      // Remove from cache
      queryClient.removeQueries({ queryKey: ['run', runId] })
    },
  })
}

// ============================================================================
// Component Management Hooks (Proxy to Core Server)
// ============================================================================

export const useComponents = (includeSchemas = true) => {
  return useQuery({
    queryKey: ['components', includeSchemas],
    queryFn: () => uiApi.listComponents(includeSchemas),
    select: (data) => data.components,
    staleTime: 10 * 60 * 1000, // 10 minutes
  })
}

// ============================================================================
// Health Check Hook
// ============================================================================

export const useHealthCheck = () => {
  return useQuery({
    queryKey: ['health'],
    queryFn: () => uiApi.healthCheck(),
    staleTime: 30 * 1000, // 30 seconds
    refetchInterval: 60 * 1000, // 1 minute
  })
}

// ============================================================================
// Helper Hooks for UI State
// ============================================================================

export const useFlowWithLabels = (name: string) => {
  const flowQuery = useFlow(name)
  const labelsQuery = useFlowLabels(name)

  return {
    flow: flowQuery.data,
    labels: labelsQuery.data || [],
    isLoading: flowQuery.isLoading || labelsQuery.isLoading,
    error: flowQuery.error || labelsQuery.error,
    refetch: () => {
      flowQuery.refetch()
      labelsQuery.refetch()
    },
  }
}

