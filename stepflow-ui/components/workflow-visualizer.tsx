'use client'

import { WorkflowVisualizerBase } from './workflow-visualizer-base'
import type { Flow } from '../stepflow-api-client/model'

// TODO: Replace with UI server API types
type StepDependency = {
  stepId: string
  dependsOn: string[]
}

type Workflow = Flow

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
  analysis?: unknown // New: dependency analysis data from API
  isDebugMode?: boolean
  onStepClick?: (stepId: string) => void
  onStepExecute?: (stepId: string) => void
}

export function WorkflowVisualizer({
  steps,
  dependencies,
  workflow,
  analysis,
  isDebugMode = false,
  onStepClick,
  onStepExecute
}: WorkflowVisualizerProps) {
  return (
    <WorkflowVisualizerBase
      steps={steps}
      dependencies={dependencies}
      workflow={workflow}
      analysis={analysis}
      isDebugMode={isDebugMode}
      showExecutionData={true} // Execution view shows runtime status
      onStepClick={onStepClick}
      onStepExecute={onStepExecute}
    />
  )
}