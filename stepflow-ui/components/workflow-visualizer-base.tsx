'use client'

import { useCallback, useMemo, useState, useEffect } from 'react'
import {
  ReactFlow,
  Node,
  Edge,
  Background,
  Controls,
  MiniMap,
  useNodesState,
  useEdgesState,
  ConnectionMode,
  NodeTypes,
  MarkerType,
  BackgroundVariant,
  Handle,
  Position
} from '@xyflow/react'
import ELK from 'elkjs/lib/elk.bundled.js'
import { Badge } from '@/components/ui/badge'
import { Card } from '@/components/ui/card'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { CheckCircle, XCircle, Clock, Activity, Play, Copy, ArrowRight, ArrowLeft, HelpCircle } from 'lucide-react'
import { cn } from '@/lib/utils'
import { StepDependency, Workflow } from '@/lib/api'
import '@xyflow/react/dist/style.css'

interface StepData {
  id: string
  name: string
  component: string
  status: 'completed' | 'running' | 'failed' | 'pending' | 'neutral'
  startTime?: string | null
  duration?: string | null
  output?: string | null
}

interface WorkflowVisualizerBaseProps {
  steps: StepData[]
  dependencies?: StepDependency[]
  workflow?: Workflow
  isDebugMode?: boolean
  showExecutionData?: boolean // Controls whether to show runtime status, times, outputs
  onStepClick?: (stepId: string) => void
  onStepExecute?: (stepId: string) => void
}

function getStatusIcon(status: string, className = "h-4 w-4") {
  switch (status) {
    case 'completed':
      return <CheckCircle className={`${className} text-green-500`} />
    case 'running':
      return <Activity className={`${className} text-blue-500 animate-pulse`} />
    case 'failed':
      return <XCircle className={`${className} text-red-500`} />
    case 'pending':
      return <Clock className={`${className} text-yellow-500`} />
    case 'neutral':
      return null // No icon for neutral status
    default:
      return <Clock className={`${className} text-gray-500`} />
  }
}

function getStatusColors(status: string, showExecutionData: boolean) {
  if (!showExecutionData) {
    // For workflow view (no execution data), use neutral colors
    return {
      border: 'border-gray-200',
      background: 'bg-white',
      text: 'text-gray-900'
    }
  }

  // For execution view, use status-based colors
  switch (status) {
    case 'completed':
      return {
        border: 'border-green-200',
        background: 'bg-green-50',
        text: 'text-green-900'
      }
    case 'running':
      return {
        border: 'border-blue-200',
        background: 'bg-blue-50',
        text: 'text-blue-900'
      }
    case 'failed':
      return {
        border: 'border-red-200',
        background: 'bg-red-50',
        text: 'text-red-900'
      }
    case 'pending':
      return {
        border: 'border-yellow-200',
        background: 'bg-yellow-50',
        text: 'text-yellow-900'
      }
    default:
      return {
        border: 'border-gray-200',
        background: 'bg-gray-50',
        text: 'text-gray-900'
      }
  }
}

interface StepNodeProps {
  data: StepData & {
    isDebugMode?: boolean
    showExecutionData?: boolean
    onStepClick?: (stepId: string) => void
    onStepExecute?: (stepId: string) => void
    inputFields?: string[]
    skipFields?: string[]
  }
}

function StepNode({ data }: StepNodeProps) {
  const colors = getStatusColors(data.status, data.showExecutionData ?? true)
  const inputFields = data.inputFields || []

  const handleClick = useCallback(() => {
    data.onStepClick?.(data.id)
  }, [data])

  const handleExecute = useCallback((e: React.MouseEvent) => {
    e.stopPropagation()
    data.onStepExecute?.(data.id)
  }, [data])

  return (
    <>
      {/* Input handles with labels */}
      {inputFields.length > 0 ? (
        inputFields.map((field, index) => (
          <Handle
            key={`input-${field}`}
            id={`input-${field}`}
            type="target"
            position={Position.Left}
            style={{
              background: '#555',
              top: `${30 + (index * 20)}%`,
              transform: 'translateY(-50%)'
            }}
          />
        ))
      ) : (
        <Handle
          type="target"
          position={Position.Left}
          style={{ background: '#555' }}
        />
      )}

      {/* Output handle (right side) */}
      <Handle
        type="source"
        position={Position.Right}
        style={{ background: '#555' }}
      />

      <Card
        className={cn(
          "min-w-48 max-w-64 cursor-pointer transition-all duration-200 hover:shadow-lg",
          colors.border,
          colors.background
        )}
        onClick={handleClick}
      >
        <div className="p-3 space-y-2">
          {/* Header with status and name */}
          <div className="flex items-center justify-between">
            <div className="flex items-center space-x-2">
              {data.showExecutionData && getStatusIcon(data.status)}
              <span className={cn("font-medium text-sm", colors.text)}>
                {data.name}
              </span>
            </div>
            {data.isDebugMode && data.status === 'pending' && (
              <button
                onClick={handleExecute}
                className="p-1 hover:bg-white/50 rounded transition-colors"
              >
                <Play className="h-3 w-3 text-gray-600" />
              </button>
            )}
          </div>

          {/* Component badge */}
          <Badge variant="outline" className="text-xs">
            {data.component}
          </Badge>

          {/* Status details - only show if execution data is enabled */}
          {data.showExecutionData && data.status !== 'pending' && data.status !== 'neutral' && (
            <div className="text-xs text-muted-foreground space-y-1">
              {data.startTime && (
                <div>Started: {data.startTime}</div>
              )}
              {data.duration && (
                <div>Duration: {data.duration}</div>
              )}
            </div>
          )}

          {/* Input field labels */}
          {inputFields.length > 0 && (
            <div className="text-xs space-y-1">
              <div className="text-muted-foreground">Inputs:</div>
              {inputFields.map((field, index) => {
                // Check if this field is a skip condition
                const isSkipCondition = data.skipFields?.includes(field)

                return (
                  <div key={field} className="flex items-center text-xs">
                    <div className="w-2 h-2 bg-gray-400 rounded-full mr-2"></div>
                    <span className="font-mono">{field}</span>
                    {isSkipCondition && (
                      <HelpCircle className="h-3 w-3 ml-1 text-orange-500" title="Skip condition" />
                    )}
                  </div>
                )
              })}
            </div>
          )}

          {/* Output preview for completed steps - only show if execution data is enabled */}
          {data.showExecutionData && data.status === 'completed' && data.output && (
            <div className="text-xs font-mono text-muted-foreground bg-white/50 p-2 rounded border">
              {data.output.length > 50
                ? `${data.output.substring(0, 50)}...`
                : data.output
              }
            </div>
          )}
        </div>
      </Card>
    </>
  )
}

function WorkflowInputNode({ data }: { data: any }) {
  return (
    <>
      <Handle
        type="source"
        position={Position.Right}
        style={{ background: '#6366f1', border: '2px solid white' }}
      />
      <div className="w-16 h-16 rounded-full bg-blue-100 border-2 border-blue-300 flex items-center justify-center shadow-lg">
        <ArrowRight className="h-6 w-6 text-blue-600" />
      </div>
      <div className="absolute -bottom-8 left-1/2 transform -translate-x-1/2 text-xs font-medium text-blue-600 whitespace-nowrap">
        Workflow Input
      </div>
    </>
  )
}

function WorkflowOutputNode({ data }: { data: any }) {
  return (
    <>
      <Handle
        type="target"
        position={Position.Left}
        id="input-result"
        style={{ background: '#10b981', border: '2px solid white' }}
      />
      <div className="w-16 h-16 rounded-full bg-green-100 border-2 border-green-300 flex items-center justify-center shadow-lg">
        <ArrowRight className="h-6 w-6 text-green-600" />
      </div>
      <div className="absolute -bottom-8 left-1/2 transform -translate-x-1/2 text-xs font-medium text-green-600 whitespace-nowrap">
        Workflow Output
      </div>
    </>
  )
}

// ELK-based automatic layout
const elk = new ELK()

async function getLayoutedElements(nodes: Node[], edges: Edge[]) {
  const nodeWidth = 250
  const nodeHeight = 120

  // Convert React Flow nodes to ELK format
  const elkNodes = nodes.map((node) => {
    let height = nodeHeight
    const inputFields = (node.data?.inputFields as string[]) || []
    if (node.type === 'stepNode' && inputFields.length > 0) {
      height = Math.max(nodeHeight, 120 + inputFields.length * 20)
    }
    if (node.type === 'workflowInputNode' || node.type === 'workflowOutputNode') {
      height = 80
    }

    return {
      id: node.id,
      width: nodeWidth,
      height: height
    }
  })

  const elkEdges = edges.map((edge) => ({
    id: edge.id,
    sources: [edge.source],
    targets: [edge.target]
  }))

  const elkGraph = {
    id: 'root',
    layoutOptions: {
      'elk.algorithm': 'layered',
      'elk.direction': 'RIGHT',
      'elk.spacing.nodeNode': '40',
      'elk.layered.spacing.nodeNodeBetweenLayers': '200',
      'elk.spacing.edgeNode': '40',
      'elk.spacing.edgeEdge': '20',
      'elk.layered.crossingMinimization.strategy': 'LAYER_SWEEP',
      'elk.layered.nodePlacement.strategy': 'BRANDES_KOEPF',
      'elk.padding': '[top=60,left=60,bottom=60,right=60]',
      'elk.separateConnectedComponents': 'false'
    },
    children: elkNodes,
    edges: elkEdges
  }

  const layoutedGraph = await elk.layout(elkGraph)

  // Apply the layout back to React Flow nodes
  nodes.forEach((node) => {
    const elkNode = layoutedGraph.children?.find((n) => n.id === node.id)
    if (elkNode) {
      node.position = {
        x: elkNode.x || 0,
        y: elkNode.y || 0
      }
    }
  })

  return { nodes, edges }
}

async function calculateLayout(steps: StepData[], dependencies?: StepDependency[], workflow?: any): Promise<{ nodes: Node[], edges: Edge[] }> {
  const stepMap = new Map(steps.map((step, index) => [step.id, { ...step, index }]))
  const stepIndexMap = new Map(steps.map((step, index) => [index, step.id]))
  let nodes: Node[] = []
  let edges: Edge[] = []

  // Track workflow input/output dependencies
  let hasWorkflowInputDeps = false
  let hasWorkflowOutputDeps = false
  const stepInputFields = new Map<string, string[]>()
  const stepWorkflowInputFields = new Map<string, string[]>()
  const stepSkipFields = new Map<string, string[]>()
  const stepsWithWorkflowOutput = new Set<string>()

  // Initialize step data
  steps.forEach((step, index) => {
    stepInputFields.set(step.id, [])
    stepSkipFields.set(step.id, [])
    stepWorkflowInputFields.set(step.id, [])
  })

  // Extract input fields from workflow definition
  if (workflow?.steps) {
    workflow.steps.forEach((workflowStep: any, index: number) => {
      const stepId = stepIndexMap.get(index)
      if (stepId && workflowStep.input && typeof workflowStep.input === 'object') {
        const fields = Object.keys(workflowStep.input)
        stepInputFields.set(stepId, fields)

        // Track workflow input fields
        const workflowInputFields: string[] = []
        Object.entries(workflowStep.input).forEach(([fieldName, inputValue]: [string, any]) => {
          if (inputValue && typeof inputValue === 'object' && inputValue.$from?.workflow === 'input') {
            workflowInputFields.push(fieldName)
            hasWorkflowInputDeps = true
          }
        })
        stepWorkflowInputFields.set(stepId, workflowInputFields)
      }
    })
  }

  // Process dependencies
  if (dependencies && dependencies.length > 0) {
    dependencies.forEach(dep => {
      if (dep.step_index > 1000000) { // Workflow output dependency
        const sourceStepId = stepIndexMap.get(dep.depends_on_step_index)
        if (sourceStepId) {
          stepsWithWorkflowOutput.add(sourceStepId)
          hasWorkflowOutputDeps = true
        }
      } else {
        // Track skip condition fields
        const targetStepId = stepIndexMap.get(dep.step_index)
        if (targetStepId && dep.dst_field.skip_if) {
          const skipFields = stepSkipFields.get(targetStepId) || []
          const fieldName = dep.dst_field.input_field || 'condition'
          if (!skipFields.includes(fieldName)) {
            skipFields.push(fieldName)
            stepSkipFields.set(targetStepId, skipFields)
          }
        }
      }
    })
  }

  // Create step nodes
  steps.forEach((step, index) => {
    const inputFields = stepInputFields.get(step.id) || []
    const workflowInputFields = stepWorkflowInputFields.get(step.id) || []
    const skipFields = stepSkipFields.get(step.id) || []
    const hasWorkflowOutput = stepsWithWorkflowOutput.has(step.id)

    nodes.push({
      id: step.id,
      type: 'stepNode',
      position: { x: 0, y: 0 },
      data: {
        ...step,
        inputFields,
        workflowInputFields,
        skipFields,
        hasWorkflowOutput
      } as unknown as Record<string, unknown>
    })
  })

  // Add workflow input node if needed
  if (hasWorkflowInputDeps) {
    nodes.push({
      id: 'workflow-input',
      type: 'workflowInputNode',
      position: { x: 0, y: 0 },
      data: {
        id: 'workflow-input',
        name: 'Workflow Input',
        inputFields: []
      } as unknown as Record<string, unknown>
    })
  }

  // Add workflow output node if needed
  if (hasWorkflowOutputDeps) {
    nodes.push({
      id: 'workflow-output',
      type: 'workflowOutputNode',
      position: { x: 0, y: 0 },
      data: {
        id: 'workflow-output',
        name: 'Workflow Output',
        inputFields: []
      } as unknown as Record<string, unknown>
    })
  }

  // Create workflow input edges
  if (hasWorkflowInputDeps && workflow?.steps) {
    workflow.steps.forEach((workflowStep: any, index: number) => {
      const targetStepId = stepIndexMap.get(index)
      if (targetStepId && workflowStep.input && typeof workflowStep.input === 'object') {
        Object.entries(workflowStep.input).forEach(([fieldName, inputValue]: [string, any]) => {
          if (inputValue && typeof inputValue === 'object' && inputValue.$from?.workflow === 'input') {
            edges.push({
              id: `edge-input-${index}-${fieldName}`,
              source: 'workflow-input',
              target: targetStepId,
              targetHandle: `input-${fieldName}`,
              type: 'smoothstep',
              style: { strokeWidth: 2, stroke: '#6366f1' },
              markerEnd: {
                type: MarkerType.ArrowClosed,
                color: '#6366f1',
                width: 20,
                height: 20
              }
            })
          }
        })
      }
    })
  }

  // Create step-to-step and workflow output edges
  if (dependencies && dependencies.length > 0) {
    dependencies.forEach(dep => {
      // Handle workflow output dependencies
      if (dep.step_index > 1000000) {
        const sourceStepId = stepIndexMap.get(dep.depends_on_step_index)
        if (sourceStepId) {
          edges.push({
            id: `edge-${sourceStepId}-to-output`,
            source: sourceStepId,
            target: 'workflow-output',
            targetHandle: 'input-result',
            type: 'smoothstep',
            style: { strokeWidth: 2, stroke: '#10b981' },
            markerEnd: {
              type: MarkerType.ArrowClosed,
              color: '#10b981',
              width: 20,
              height: 20
            }
          })
        }
        return
      }

      const sourceStepId = stepIndexMap.get(dep.depends_on_step_index)
      const targetStepId = stepIndexMap.get(dep.step_index)

      if (sourceStepId && targetStepId) {
        const isOptional = dep.skip_action.action === 'use_default'
        const isSkipCondition = dep.dst_field.skip_if === true

        let edgeStyle = { strokeWidth: 2, stroke: '#94a3b8' }

        if (isSkipCondition) {
          edgeStyle = { strokeWidth: 2, stroke: '#f97316' }
        } else if (isOptional) {
          edgeStyle = { strokeWidth: 2, stroke: '#94a3b8', strokeDasharray: '5,5' }
        }

        edges.push({
          id: `edge-${dep.depends_on_step_index}-${dep.step_index}-${dep.dst_field.input_field || 'default'}`,
          source: sourceStepId,
          target: targetStepId,
          targetHandle: dep.dst_field.input_field ? `input-${dep.dst_field.input_field}` : undefined,
          type: 'smoothstep',
          style: edgeStyle,
          markerEnd: {
            type: MarkerType.ArrowClosed,
            color: edgeStyle.stroke,
            width: 20,
            height: 20
          }
        })
      }
    })
  }

  return await getLayoutedElements(nodes, edges)
}

function StatusLegend({ showExecutionData }: { showExecutionData?: boolean }) {
  if (!showExecutionData) return null

  const statusItems = [
    { status: 'completed', label: 'Completed', icon: CheckCircle, color: 'text-green-500' },
    { status: 'running', label: 'Running', icon: Activity, color: 'text-blue-500' },
    { status: 'failed', label: 'Failed', icon: XCircle, color: 'text-red-500' },
    { status: 'pending', label: 'Pending', icon: Clock, color: 'text-yellow-500' },
  ]

  return (
    <Card className="absolute top-4 right-4 z-10 p-3">
      <div className="text-xs font-medium text-muted-foreground mb-2">Status Legend</div>
      <div className="space-y-1">
        {statusItems.map(({ status, label, icon: Icon, color }) => (
          <div key={status} className="flex items-center space-x-2 text-xs">
            <Icon className={cn("h-3 w-3", color)} />
            <span>{label}</span>
          </div>
        ))}
      </div>
    </Card>
  )
}

function StepDefinitionDialog({
  stepId,
  workflow,
  isOpen,
  onClose
}: {
  stepId: string | null
  workflow?: Workflow
  isOpen: boolean
  onClose: () => void
}) {
  const step = workflow?.steps.find(s => s.id === stepId)

  if (!step) {
    return null
  }

  const copyToClipboard = (text: string) => {
    navigator.clipboard.writeText(text)
  }

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-2xl max-h-[80vh] overflow-auto">
        <DialogHeader>
          <DialogTitle>Step Definition: {step.id}</DialogTitle>
          <DialogDescription>
            Complete configuration for this workflow step
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          {/* Basic Information */}
          <div className="space-y-3">
            <h3 className="text-sm font-medium text-gray-900">Basic Information</h3>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <span className="text-xs text-muted-foreground">Step ID:</span>
                <div className="font-mono text-sm">{step.id}</div>
              </div>
              <div>
                <span className="text-xs text-muted-foreground">Component:</span>
                <div className="font-mono text-sm">{step.component}</div>
              </div>
            </div>
          </div>

          {/* Input Configuration */}
          {step.input && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-medium text-gray-900">Input Configuration</h3>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => copyToClipboard(JSON.stringify(step.input, null, 2))}
                >
                  <Copy className="h-3 w-3 mr-1" />
                  Copy
                </Button>
              </div>
              <div className="bg-muted rounded-lg p-4">
                <pre className="text-xs font-mono whitespace-pre-wrap overflow-x-auto">
                  {JSON.stringify(step.input, null, 2)}
                </pre>
              </div>
            </div>
          )}

          {/* Dependencies */}
          {step.depends_on && step.depends_on.length > 0 && (
            <div className="space-y-3">
              <h3 className="text-sm font-medium text-gray-900">Dependencies</h3>
              <div className="flex flex-wrap gap-2">
                {step.depends_on.map((dep, index) => (
                  <Badge key={index} variant="outline">
                    {dep}
                  </Badge>
                ))}
              </div>
            </div>
          )}

          {/* Component Configuration */}
          {step.config && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <h3 className="text-sm font-medium text-gray-900">Component Configuration</h3>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => copyToClipboard(JSON.stringify(step.config, null, 2))}
                >
                  <Copy className="h-3 w-3 mr-1" />
                  Copy
                </Button>
              </div>
              <div className="bg-muted rounded-lg p-4">
                <pre className="text-xs font-mono whitespace-pre-wrap overflow-x-auto">
                  {JSON.stringify(step.config, null, 2)}
                </pre>
              </div>
            </div>
          )}

          {/* Full Step Definition */}
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <h3 className="text-sm font-medium text-gray-900">Complete Step Definition</h3>
              <Button
                variant="outline"
                size="sm"
                onClick={() => copyToClipboard(JSON.stringify(step, null, 2))}
              >
                <Copy className="h-3 w-3 mr-1" />
                Copy All
              </Button>
            </div>
            <div className="bg-muted rounded-lg p-4">
              <pre className="text-xs font-mono whitespace-pre-wrap overflow-x-auto">
                {JSON.stringify(step, null, 2)}
              </pre>
            </div>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

// Define nodeTypes outside component to avoid React Flow warning
const nodeTypes: NodeTypes = {
  stepNode: StepNode,
  workflowInputNode: WorkflowInputNode,
  workflowOutputNode: WorkflowOutputNode
}

export function WorkflowVisualizerBase({
  steps,
  dependencies,
  workflow,
  isDebugMode = false,
  showExecutionData = true,
  onStepClick,
  onStepExecute
}: WorkflowVisualizerBaseProps) {
  const [selectedStepId, setSelectedStepId] = useState<string | null>(null)
  const [isDialogOpen, setIsDialogOpen] = useState(false)

  const handleStepClick = useCallback((stepId: string) => {
    setSelectedStepId(stepId)
    setIsDialogOpen(true)
    onStepClick?.(stepId)
  }, [onStepClick])

  // Memoize layout calculation input to prevent unnecessary recalculations
  const layoutInputs = useMemo(() => ({
    steps,
    dependencies,
    workflow,
    isDebugMode,
    showExecutionData
  }), [steps, dependencies, workflow, isDebugMode, showExecutionData])

  const [layoutedNodes, setLayoutedNodes] = useState<Node[]>([])
  const [layoutedEdges, setLayoutedEdges] = useState<Edge[]>([])
  const [isLayouting, setIsLayouting] = useState(false)

  // Handle async layout calculation with memoization for performance
  useEffect(() => {
    if (layoutInputs.steps.length === 0) return

    const runLayout = async () => {
      setIsLayouting(true)
      try {
        // Use a timeout to prevent blocking the UI during layout calculation
        await new Promise(resolve => setTimeout(resolve, 10))
        
        const { nodes, edges } = await calculateLayout(
          layoutInputs.steps, 
          layoutInputs.dependencies, 
          layoutInputs.workflow
        )

        // Add callback props to node data
        const nodesWithCallbacks = nodes.map(node => ({
          ...node,
          data: {
            ...node.data,
            isDebugMode: layoutInputs.isDebugMode,
            showExecutionData: layoutInputs.showExecutionData,
            onStepClick: handleStepClick,
            onStepExecute
          }
        }))

        setLayoutedNodes(nodesWithCallbacks)
        setLayoutedEdges(edges)
      } catch (error) {
        console.error('Layout calculation failed:', error)
      } finally {
        setIsLayouting(false)
      }
    }

    runLayout()
  }, [layoutInputs, handleStepClick, onStepExecute])

  const [nodes, setNodes, onNodesChange] = useNodesState([])
  const [edges, setEdges, onEdgesChange] = useEdgesState([])

  // Update nodes and edges when layout is complete
  useEffect(() => {
    if (layoutedNodes.length > 0) {
      setNodes(layoutedNodes)
    }
  }, [layoutedNodes, setNodes])

  useEffect(() => {
    if (layoutedEdges.length > 0) {
      setEdges(layoutedEdges)
    }
  }, [layoutedEdges, setEdges])

  return (
    <div className="h-96 w-full border rounded-lg bg-gray-50/50 relative">
      <StatusLegend showExecutionData={showExecutionData} />
      <StepDefinitionDialog
        stepId={selectedStepId}
        workflow={workflow}
        isOpen={isDialogOpen}
        onClose={() => setIsDialogOpen(false)}
      />
      {isLayouting && (
        <div className="absolute inset-0 flex items-center justify-center bg-white/80 z-50">
          <div className="text-sm text-gray-600">Calculating layout...</div>
        </div>
      )}
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodeTypes={nodeTypes}
        connectionMode={ConnectionMode.Strict}
        fitView
        fitViewOptions={{ padding: 0.2, includeHiddenNodes: false }}
        minZoom={0.1}
        maxZoom={2}
        nodesDraggable={false}
        nodesConnectable={false}
        elementsSelectable={true}
      >
        <Background
          variant={BackgroundVariant.Dots}
          gap={20}
          size={1}
          color="#e2e8f0"
        />
        <Controls
          position="bottom-right"
          className="bg-white border shadow-sm rounded-lg"
        />
      </ReactFlow>
    </div>
  )
}