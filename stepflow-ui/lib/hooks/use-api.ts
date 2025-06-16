import { useQuery, useMutation } from '@tanstack/react-query'
import {
  ExecutionApi,
  WorkflowApi,
  ComponentApi,
  HealthApi,
  Configuration,
  ExecutionDetailsResponse,
  StepExecutionResponse,
  WorkflowResponse,
  WorkflowDependenciesResponse,
  ListExecutionsResponse,
  ListComponentsResponse,
  ListWorkflowNamesResponse,
  WorkflowsByNameResponse,
  ListLabelsResponse,
  WorkflowLabelResponse,
  Step as WorkflowStep,
  Flow as Workflow,
  CreateExecutionRequest,
  ExecuteWorkflowRequest
} from '../../api-client'

// Type exports for compatibility
export type {
  Flow as Workflow,
  StepExecutionResponse as StepExecution
} from '../../api-client'

const basePath = process.env.STEPFLOW_API_URL || 'http://localhost:7837/api/v1';
const config = new Configuration({ basePath });

const executionApi = new ExecutionApi(config);
const workflowApi = new WorkflowApi(config);
const componentApi = new ComponentApi(config);
const healthApi = new HealthApi(config);

// Health
export const useHealth = () => {
  return useQuery({
    queryKey: ['health'],
    queryFn: () => healthApi.healthCheck(),
    refetchInterval: 30000,
    retry: 1,
  })
}

// Executions
export const useExecutions = () => {
  return useQuery({
    queryKey: ['executions'],
    queryFn: () => executionApi.listExecutions({}),
    select: (data: ListExecutionsResponse) => data.executions,
    refetchInterval: 5000,
  })
}

export const useExecution = (executionId: string) => {
  return useQuery({
    queryKey: ['execution', executionId],
    queryFn: () => executionApi.getExecution({ executionId }),
    enabled: !!executionId,
    refetchInterval: (query) => {
      const execution = query.state.data as ExecutionDetailsResponse | undefined
      return execution?.status === 'running' ? 2000 : 10000
    },
  })
}

export const useExecutionSteps = (executionId: string) => {
  return useQuery({
    queryKey: ['executionSteps', executionId],
    queryFn: () => executionApi.getExecutionSteps({ executionId }),
    enabled: !!executionId,
    select: (data) => data.steps,
  })
}

export const useExecutionWorkflow = (executionId: string) => {
  return useQuery({
    queryKey: ['executionWorkflow', executionId],
    queryFn: () => executionApi.getExecutionWorkflow({ executionId }),
    enabled: !!executionId,
    staleTime: 5 * 60 * 1000,
  })
}

export const useWorkflowDependencies = (workflowHash: string) => {
  return useQuery({
    queryKey: ['workflowDependencies', workflowHash],
    queryFn: () => workflowApi.getWorkflowDependencies({ workflowHash }),
    enabled: !!workflowHash,
    staleTime: 5 * 60 * 1000,
  })
}

export const useWorkflowNames = () => {
  return useQuery({
    queryKey: ['workflowNames'],
    queryFn: () => workflowApi.listWorkflowNames(),
    select: (data: ListWorkflowNamesResponse) => data.names,
  })
}

export const useWorkflowsByName = (name: string) => {
  return useQuery({
    queryKey: ['workflowsByName', name],
    queryFn: () => workflowApi.getWorkflowsByName({ name }),
    enabled: !!name,
    staleTime: 5 * 60 * 1000,
  })
}

export const useLatestWorkflowByName = (name: string) => {
  return useQuery({
    queryKey: ['latestWorkflowByName', name],
    queryFn: () => workflowApi.getLatestWorkflowByName({ name }),
    enabled: !!name,
    staleTime: 5 * 60 * 1000,
  })
}

export const useWorkflowByLabel = (name: string, label: string) => {
  return useQuery({
    queryKey: ['workflowByLabel', name, label],
    queryFn: () => workflowApi.getWorkflowByLabel({ name, label }),
    enabled: !!name && !!label,
  })
}

export const useLabelsForName = (name: string) => {
  return useQuery({
    queryKey: ['labelsForName', name],
    queryFn: () => workflowApi.listLabelsForName({ name }),
    enabled: !!name,
  })
}

export const useComponents = (includeSchemas: boolean = true) => {
  return useQuery({
    queryKey: ['components', includeSchemas],
    queryFn: () => componentApi.listComponents({ includeSchemas }),
    select: (data: ListComponentsResponse) => data.components,
  })
}

// Helper for workflow steps
export const getWorkflowSteps = (workflowResponse: WorkflowResponse | undefined): WorkflowStep[] => {
  if (!workflowResponse?.workflow?.steps) return []
  return workflowResponse.workflow.steps.map((workflowStep: WorkflowStep, index: number) => ({ ...workflowStep, index }))
}

// Helper for step executions
export const getStepExecutions = (steps: StepExecutionResponse[] = []) => {
  return steps.map((step: StepExecutionResponse) => ({
    ...step,
    stepIndex: step.stepIndex
  }))
}

// Execute workflow mutation
export const useExecuteWorkflow = () => {
  return useMutation({
    mutationFn: async ({ workflow, input, debug = false }: {
      workflow: Workflow;
      input: Record<string, unknown>;
      debug?: boolean;
    }) => {
      const request: CreateExecutionRequest = {
        workflow,
        input,
        debug
      }
      return executionApi.createExecution({ createExecutionRequest: request })
    }
  })
}

// Execute workflow by name mutation
export const useExecuteWorkflowByName = () => {
  return useMutation({
    mutationFn: async ({ name, data }: {
      name: string;
      data: ExecuteWorkflowRequest;
    }) => {
      return workflowApi.executeWorkflowByName({ name, executeWorkflowRequest: data })
    }
  })
}

// Execute workflow by label mutation
export const useExecuteWorkflowByLabel = () => {
  return useMutation({
    mutationFn: async ({ name, label, data }: {
      name: string;
      label: string;
      data: ExecuteWorkflowRequest;
    }) => {
      return workflowApi.executeWorkflowByLabel({ name, label, executeWorkflowRequest: data })
    }
  })
}

// Transform step executions for visualizer
export const transformStepsForVisualizer = (
  steps: StepExecutionResponse[],
  workflow?: Workflow
) => {
  if (!workflow?.steps) return []

  return workflow.steps.map((workflowStep) => {
    const execution = steps.find(s => s.stepId === workflowStep.id)

    let status: 'completed' | 'running' | 'failed' | 'pending' = 'pending'
    let output: string | null = null

    if (execution) {
      switch (execution.state) {
        case 'completed':
          status = 'completed'
          const result = execution.result as any;
          if (result?.outcome === 'success') {
            output = JSON.stringify(result.result)
          } else if (result?.outcome === 'failed') {
            const error = result.error
            output = `Error ${error?.code || 'Unknown'}: ${error?.message || 'Unknown error'}`
          } else if (result?.outcome === 'skipped') {
            output = 'Skipped'
          }
          break
        case 'failed':
          status = 'failed'
          const failedResult = execution.result as any;
          if (failedResult?.outcome === 'failed') {
            const error = failedResult.error
            output = `Error ${error?.code || 'Unknown'}: ${error?.message || 'Unknown error'}`
          }
          break
        case 'running':
          status = 'running'
          break
        case 'skipped':
          status = 'completed'
          output = 'Skipped'
          break
      }
    }

    return {
      id: workflowStep.id,
      name: workflowStep.id,
      component: workflowStep.component,
      status,
      dependencies: [], // Dependencies not available in current Step model
      startTime: null,
      duration: null,
      output
    }
  })
}