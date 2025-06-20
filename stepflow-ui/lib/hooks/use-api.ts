// DEPRECATED: This file contains legacy hooks that directly call the core server
// New code should use hooks from './use-workflow-api' instead

// Re-export new hooks for migration
export * from './use-flow-api'

// All legacy hooks are deprecated and disabled
// Use the new hooks from use-workflow-api instead

/** @deprecated Use useHealthCheck from use-workflow-api instead */
export const useHealth = () => {
  throw new Error('useHealth is deprecated - use useHealthCheck from use-workflow-api instead')
}

/** @deprecated Legacy hook disabled */
export const useExecutions = () => {
  throw new Error('useExecutions is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useExecution = () => {
  throw new Error('useExecution is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useExecutionSteps = () => {
  throw new Error('useExecutionSteps is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useExecutionWorkflow = () => {
  throw new Error('useExecutionWorkflow is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useWorkflowDependencies = () => {
  throw new Error('useWorkflowDependencies is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useWorkflowNames = () => {
  throw new Error('useWorkflowNames is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useWorkflowsByName = () => {
  throw new Error('useWorkflowsByName is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useLatestWorkflowByName = () => {
  throw new Error('useLatestWorkflowByName is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useWorkflowByLabel = () => {
  throw new Error('useWorkflowByLabel is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useLabelsForName = () => {
  throw new Error('useLabelsForName is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useComponents = () => {
  throw new Error('useComponents is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const getWorkflowSteps = () => {
  throw new Error('getWorkflowSteps is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const getStepExecutions = () => {
  throw new Error('getStepExecutions is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useExecuteWorkflow = () => {
  throw new Error('useExecuteWorkflow is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useExecuteWorkflowByName = () => {
  throw new Error('useExecuteWorkflowByName is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const useExecuteWorkflowByLabel = () => {
  throw new Error('useExecuteWorkflowByLabel is deprecated - use new UI server API hooks instead')
}

/** @deprecated Legacy hook disabled */
export const transformStepsForVisualizer = () => {
  throw new Error('transformStepsForVisualizer is deprecated - use new UI server API hooks instead')
}