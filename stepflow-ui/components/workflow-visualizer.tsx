'use client'

import { useCallback, useMemo } from 'react'
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
  BackgroundVariant
} from '@xyflow/react'
import { Badge } from '@/components/ui/badge'
import { Card } from '@/components/ui/card'
import { CheckCircle, XCircle, Clock, Activity, Play } from 'lucide-react'
import { cn } from '@/lib/utils'
import '@xyflow/react/dist/style.css'

interface StepData {
  id: string
  name: string
  component: string
  status: 'completed' | 'running' | 'failed' | 'pending'
  dependencies: string[]
  startTime?: string | null
  duration?: string | null
  output?: string | null
}

interface WorkflowVisualizerProps {
  steps: StepData[]
  isDebugMode?: boolean
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
    default:
      return <Clock className={`${className} text-gray-500`} />
  }
}

function getStatusColors(status: string) {
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
    onStepClick?: (stepId: string) => void
    onStepExecute?: (stepId: string) => void
  }
}

function StepNode({ data }: StepNodeProps) {
  const colors = getStatusColors(data.status)
  
  const handleClick = useCallback(() => {
    data.onStepClick?.(data.id)
  }, [data])

  const handleExecute = useCallback((e: React.MouseEvent) => {
    e.stopPropagation()
    data.onStepExecute?.(data.id)
  }, [data])

  return (
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
            {getStatusIcon(data.status)}
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

        {/* Status details */}
        {data.status !== 'pending' && (
          <div className="text-xs text-muted-foreground space-y-1">
            {data.startTime && (
              <div>Started: {data.startTime}</div>
            )}
            {data.duration && (
              <div>Duration: {data.duration}</div>
            )}
          </div>
        )}

        {/* Output preview for completed steps */}
        {data.status === 'completed' && data.output && (
          <div className="text-xs font-mono text-muted-foreground bg-white/50 p-2 rounded border">
            {data.output.length > 50 
              ? `${data.output.substring(0, 50)}...`
              : data.output
            }
          </div>
        )}
      </div>
    </Card>
  )
}

// Auto-layout algorithm for positioning nodes
function calculateLayout(steps: StepData[]): { nodes: Node[], edges: Edge[] } {
  const stepMap = new Map(steps.map(step => [step.id, step]))
  const nodes: Node[] = []
  const edges: Edge[] = []
  
  // Build dependency graph
  const dependents = new Map<string, string[]>()
  const dependsOn = new Map<string, string[]>()
  
  steps.forEach(step => {
    dependsOn.set(step.id, step.dependencies)
    step.dependencies.forEach(dep => {
      if (!dependents.has(dep)) {
        dependents.set(dep, [])
      }
      dependents.get(dep)!.push(step.id)
    })
  })
  
  // Calculate levels (topological layers)
  const levels = new Map<string, number>()
  const visited = new Set<string>()
  
  function calculateLevel(stepId: string): number {
    if (visited.has(stepId)) {
      return levels.get(stepId) || 0
    }
    
    visited.add(stepId)
    const deps = dependsOn.get(stepId) || []
    
    if (deps.length === 0) {
      levels.set(stepId, 0)
      return 0
    }
    
    const maxDepLevel = Math.max(...deps.map(dep => calculateLevel(dep)))
    const level = maxDepLevel + 1
    levels.set(stepId, level)
    return level
  }
  
  steps.forEach(step => calculateLevel(step.id))
  
  // Group steps by level
  const levelGroups = new Map<number, string[]>()
  levels.forEach((level, stepId) => {
    if (!levelGroups.has(level)) {
      levelGroups.set(level, [])
    }
    levelGroups.get(level)!.push(stepId)
  })
  
  // Position nodes
  const nodeSpacing = { x: 280, y: 120 }
  const startY = 50
  
  levelGroups.forEach((stepIds, level) => {
    const levelHeight = stepIds.length * nodeSpacing.y
    const startX = level * nodeSpacing.x + 50
    
    stepIds.forEach((stepId, index) => {
      const step = stepMap.get(stepId)!
      const y = startY + (index * nodeSpacing.y) - (levelHeight / 2) + (nodeSpacing.y / 2)
      
      nodes.push({
        id: stepId,
        type: 'stepNode',
        position: { x: startX, y },
        data: step as unknown as Record<string, unknown>,
        dragHandle: '.drag-handle'
      })
    })
  })
  
  // Create edges
  steps.forEach(step => {
    step.dependencies.forEach(dep => {
      edges.push({
        id: `${dep}-${step.id}`,
        source: dep,
        target: step.id,
        type: 'smoothstep',
        style: { strokeWidth: 2, stroke: '#94a3b8' },
        markerEnd: {
          type: MarkerType.ArrowClosed,
          color: '#94a3b8',
          width: 20,
          height: 20
        }
      })
    })
  })
  
  return { nodes, edges }
}

function StatusLegend() {
  const statusItems = [
    { status: 'completed', label: 'Completed', icon: CheckCircle, color: 'text-green-500' },
    { status: 'running', label: 'Running', icon: Activity, color: 'text-blue-500' },
    { status: 'failed', label: 'Failed', icon: XCircle, color: 'text-red-500' },
    { status: 'pending', label: 'Pending', icon: Clock, color: 'text-yellow-500' },
  ]

  return (
    <Card className="absolute top-4 left-4 z-10 p-3">
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

export function WorkflowVisualizer({ 
  steps, 
  isDebugMode = false, 
  onStepClick, 
  onStepExecute 
}: WorkflowVisualizerProps) {
  const { nodes: initialNodes, edges: initialEdges } = useMemo(() => {
    const { nodes, edges } = calculateLayout(steps)
    
    // Add callback props to node data
    return {
      nodes: nodes.map(node => ({
        ...node,
        data: {
          ...node.data,
          isDebugMode,
          onStepClick,
          onStepExecute
        }
      })),
      edges
    }
  }, [steps, isDebugMode, onStepClick, onStepExecute])

  const [nodes, , onNodesChange] = useNodesState(initialNodes)
  const [edges, , onEdgesChange] = useEdgesState(initialEdges)

  const nodeTypes: NodeTypes = useMemo(() => ({
    stepNode: StepNode
  }), [])

  return (
    <div className="h-96 w-full border rounded-lg bg-gray-50/50 relative">
      <StatusLegend />
      <ReactFlow
        nodes={nodes}
        edges={edges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        nodeTypes={nodeTypes}
        connectionMode={ConnectionMode.Strict}
        fitView
        fitViewOptions={{ padding: 0.2 }}
        minZoom={0.3}
        maxZoom={2}
        defaultEdgeOptions={{
          type: 'smoothstep',
          style: { strokeWidth: 2, stroke: '#94a3b8' }
        }}
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
        <MiniMap 
          position="bottom-left"
          className="bg-white border shadow-sm rounded-lg"
          maskColor="rgba(0,0,0,0.1)"
          nodeColor={(node) => {
            const status = (node.data as unknown as StepData).status
            const colors = getStatusColors(status)
            return colors.background.includes('green') ? '#22c55e' :
                   colors.background.includes('blue') ? '#3b82f6' :
                   colors.background.includes('red') ? '#ef4444' :
                   colors.background.includes('yellow') ? '#eab308' : '#6b7280'
          }}
        />
      </ReactFlow>
    </div>
  )
}