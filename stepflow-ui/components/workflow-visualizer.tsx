'use client'

import { WorkflowVisualizerBase } from './workflow-visualizer-base'
import { StepDependency, Workflow } from '@/lib/api'

interface StepData {
  id: string
  name: string
  component: string
  status: 'completed' | 'running' | 'failed' | 'pending' | 'neutral'
  startTime?: string | null
  duration?: string | null
  output?: string | null
}

interface WorkflowVisualizerProps {
  steps: StepData[]
  dependencies?: StepDependency[]
  workflow?: Workflow
  isDebugMode?: boolean
  onStepClick?: (stepId: string) => void
  onStepExecute?: (stepId: string) => void
}

export function WorkflowVisualizer({
  steps,
  dependencies,
  workflow,
  isDebugMode = false,
  onStepClick,
  onStepExecute
}: WorkflowVisualizerProps) {
  return (
    <WorkflowVisualizerBase
      steps={steps}
      dependencies={dependencies}
      workflow={workflow}
      isDebugMode={isDebugMode}
      showExecutionData={true} // Execution view shows runtime status
      onStepClick={onStepClick}
      onStepExecute={onStepExecute}
    />
  )
}