'use client'

import Link from 'next/link'
import { useParams, useRouter, useSearchParams } from 'next/navigation'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Progress } from '@/components/ui/progress'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip'
import {
  ArrowLeft,
  Activity,
  CheckCircle,
  XCircle,
  Clock,
  Play,
  Pause,
  RotateCcw,
  Eye,
  Bug,
  Download,
  Copy,
  Loader2,
  AlertCircle
} from 'lucide-react'
import { WorkflowVisualizer } from '@/components/workflow-visualizer'
import { useRun, useRunSteps, useRunWorkflow, useFlow } from '@/lib/hooks/use-flow-api'
import type { StepExecution } from '@/lib/api-types'
import type { Flow } from '@/stepflow-api-client'
import { convertAnalysisToDependencies, type FlowAnalysis, type StepDependency } from '@/lib/dependency-types'

// Helper functions
const formatDate = (dateString: string) => {
  return new Date(dateString).toLocaleString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

// Note: convertAnalysisToDependencies is now imported from @/lib/dependency-types

// Transform step executions for the visualizer
const transformStepsForVisualizer = (steps: StepExecution[], workflow?: { steps?: Array<{ id: string; component: string }> }) => {
  if (!workflow?.steps) return []
  
  return workflow.steps.map((workflowStep, index) => {
    const stepExecution = steps.find(s => s.stepIndex === index)
    
    // Map execution state to visualizer status
    let status: 'completed' | 'running' | 'failed' | 'pending' | 'neutral' = 'pending'
    let startTime: string | null = null
    let duration: string | null = null
    let output: string | null = null
    
    if (stepExecution) {
      switch (stepExecution.state) {
        case 'completed':
          // Check if result has Failed property for new API structure
          if (stepExecution.result && typeof stepExecution.result === 'object' && 'Failed' in stepExecution.result) {
            status = 'failed'
          } else {
            status = 'completed'
          }
          break
        case 'running':
          status = 'running'
          break
        case 'failed':
          status = 'failed'
          break
        case 'skipped':
          status = 'completed' // Show as completed but with different output
          output = 'Skipped'
          break
        default:
          status = 'pending'
      }
      
      if (stepExecution.startedAt) {
        startTime = formatDate(stepExecution.startedAt)
      }
      
      if (stepExecution.startedAt && stepExecution.completedAt) {
        const start = new Date(stepExecution.startedAt)
        const end = new Date(stepExecution.completedAt)
        const durationMs = end.getTime() - start.getTime()
        duration = `${(durationMs / 1000).toFixed(2)}s`
      }
      
      if (stepExecution.result && typeof stepExecution.result === 'object' && 'Success' in stepExecution.result) {
        output = JSON.stringify(stepExecution.result.Success)
      } else if (stepExecution.result && typeof stepExecution.result === 'object' && 'Failed' in stepExecution.result) {
        output = JSON.stringify(stepExecution.result.Failed)
      }
    }
    
    return {
      id: workflowStep.id,
      name: workflowStep.id,
      component: workflowStep.component,
      status,
      startTime,
      duration,
      output,
    }
  })
}

const getDuration = (createdAt: string, completedAt?: string) => {
  const start = new Date(createdAt)
  const end = completedAt ? new Date(completedAt) : new Date()
  const diffMs = end.getTime() - start.getTime()
  const diffSecs = Math.floor(diffMs / 1000)
  const diffMins = Math.floor(diffMs / 60000)
  const diffHours = Math.floor(diffMs / 3600000)

  if (diffSecs < 60) return `${diffSecs}s`
  if (diffMins < 60) return `${diffMins}m ${diffSecs % 60}s`
  if (diffHours < 24) return `${diffHours}h ${diffMins % 60}m`
  return `${Math.floor(diffHours / 24)}d ${diffHours % 24}h`
}

function getStatusIcon(status: string, className = "h-4 w-4") {
  switch (status.toLowerCase()) {
    case 'completed':
      return <CheckCircle className={`${className} text-green-500`} />
    case 'running':
      return <Activity className={`${className} text-blue-500 animate-pulse`} />
    case 'failed':
      return <XCircle className={`${className} text-red-500`} />
    case 'cancelled':
      return <XCircle className={`${className} text-orange-500`} />
    case 'paused':
      return <Pause className={`${className} text-yellow-500`} />
    case 'pending':
      return <Clock className={`${className} text-yellow-500`} />
    default:
      return <Clock className={`${className} text-gray-500`} />
  }
}

function getStatusBadge(status: string) {
  const variants: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
    completed: 'default',
    running: 'secondary',
    failed: 'destructive',
    cancelled: 'destructive',
    paused: 'outline',
    pending: 'outline',
  }

  return (
    <Badge variant={variants[status.toLowerCase()] || 'outline'}>
      {status}
    </Badge>
  )
}

function StepOutputDialog({ step }: { step: { id: string; name: string; component: string; output: string | null } }) {
  return (
    <Dialog>
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm" disabled={!step.output}>
          <Eye className="h-4 w-4" />
        </Button>
      </DialogTrigger>
      <DialogContent className="max-w-4xl max-h-[80vh] overflow-auto">
        <DialogHeader>
          <DialogTitle>Step Output: {step.name}</DialogTitle>
          <DialogDescription>
            Component: {step.component}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="flex items-center space-x-2">
            <Button variant="outline" size="sm">
              <Copy className="mr-2 h-4 w-4" />
              Copy
            </Button>
            <Button variant="outline" size="sm">
              <Download className="mr-2 h-4 w-4" />
              Download
            </Button>
          </div>
          <div className="bg-muted rounded-lg p-4">
            <pre className="text-sm font-mono whitespace-pre-wrap">
              {step.output ? JSON.stringify(JSON.parse(step.output), null, 2) : 'No output available'}
            </pre>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  )
}

function WorkflowVisualization({
  visualizerSteps,
  dependencies,
  workflow,
  analysisData,
  isDebugMode,
  isLoading
}: {
  visualizerSteps: Array<{
    id: string
    name: string
    component: string
    status: 'completed' | 'running' | 'failed' | 'pending'
    startTime: string | null
    duration: string | null
    output: string | null
  }>,
  dependencies?: StepDependency[],
  workflow?: Flow,
  analysisData?: FlowAnalysis,
  isDebugMode: boolean,
  isLoading: boolean
}) {
  const handleStepClick = (stepId: string) => {
    const step = visualizerSteps.find(s => s.id === stepId)
    if (step?.output) {
      // Open step output dialog - for now just log
      console.log('Opening step details for:', stepId)
    }
  }

  const handleStepExecute = (stepId: string) => {
    console.log('Executing step:', stepId)
    // TODO: Implement step execution logic
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-96">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (visualizerSteps.length === 0) {
    return (
      <div className="flex items-center justify-center h-96 text-muted-foreground">
        <div className="text-center">
          <Activity className="h-12 w-12 mx-auto mb-2" />
          <p className="text-lg font-medium">No workflow data available</p>
          <p className="text-sm">Workflow structure will appear here when available</p>
        </div>
      </div>
    )
  }

  return (
    <WorkflowVisualizer
      steps={visualizerSteps}
      dependencies={dependencies}
      workflow={workflow}
      analysis={analysisData}
      isDebugMode={isDebugMode}
      onStepClick={handleStepClick}
      onStepExecute={handleStepExecute}
    />
  )
}

export default function RunDetailsPage() {
  const params = useParams()
  const router = useRouter()
  const searchParams = useSearchParams()
  const executionId = params.id as string

  // API calls - use new UI server API
  const { data: execution, isLoading: executionLoading, error: executionError, refetch: refetchExecution } = useRun(executionId)
  const { data: steps, isLoading: stepsLoading, error: stepsError, refetch: refetchSteps } = useRunSteps(executionId)
  const { data: workflowData, isLoading: workflowLoading, error: workflowError, refetch: refetchWorkflow } = useRunWorkflow(executionId)
  
  // Get flow analysis data if this is a named workflow (not ad-hoc)
  const workflowName = workflowData?.workflowName || execution?.flowName
  const { data: flowDetail, isLoading: flowDetailLoading } = useFlow(workflowName || '')
  
  // Extract analysis data from flow detail
  const analysisData = flowDetail?.analysis

  // Early return if execution ID is not available yet (during Next.js hydration)
  if (!executionId) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    )
  }

  // Get tab from URL or default to 'overview'
  const validTabs = ['overview', 'steps', 'visualization'] as const
  const tabFromUrl = searchParams.get('tab')
  const selectedTab: string = tabFromUrl && validTabs.includes(tabFromUrl as typeof validTabs[number])
    ? tabFromUrl
    : 'overview'

  // Function to update tab in URL
  const setSelectedTab = (tab: string) => {
    const newParams = new URLSearchParams(searchParams.toString())
    if (tab === 'overview') {
      // Remove tab parameter for default tab to keep URL clean
      newParams.delete('tab')
    } else {
      newParams.set('tab', tab)
    }
    const newUrl = newParams.toString() ? `?${newParams.toString()}` : ''
    router.replace(`/runs/${executionId}${newUrl}`, { scroll: false })
  }

  const refetchAll = () => {
    refetchExecution()
    refetchSteps()
    refetchWorkflow()
  }

  // Transform data for display
  const flowName = workflowData?.workflowName || execution?.flowName || 'Ad-hoc Run'
  const isDebugMode = execution?.debugMode || false
  const totalSteps = workflowData?.flow?.steps?.length || 0
  const completedSteps = steps?.filter(step => {
    const result = step.result;
    if (!result) return false;
    if (typeof result === 'string') return true;
    if (typeof result === 'object') {
      return 'Success' in result || 'Failed' in result;
    }
    return false;
  }).length || 0
  const runningSteps = steps?.filter(step =>
    step.state === 'running'
  ).length || 0
  const pendingSteps = totalSteps - completedSteps - runningSteps
  const progress = totalSteps > 0 ? Math.round((completedSteps / totalSteps) * 100) : 0

  // Transform steps for visualizer
  const visualizerSteps = transformStepsForVisualizer(steps || [], workflowData?.flow)

  // Convert analysis data to dependencies for visualizer
  const workflowDependencies = analysisData ? convertAnalysisToDependencies(analysisData) : undefined

  // Error handling
  if (executionError || stepsError || workflowError) {
    return (
      <div className="space-y-6">
        <div className="flex items-center gap-4">
          <Button variant="ghost" size="sm" asChild>
            <Link href="/runs">
              <ArrowLeft className="h-4 w-4" />
            </Link>
          </Button>
        </div>

        <div className="flex items-center justify-center h-64">
          <div className="text-center">
            <AlertCircle className="h-8 w-8 text-red-500 mx-auto mb-2" />
            <h2 className="text-lg font-semibold text-gray-900 mb-1">
              Failed to load run
            </h2>
            <p className="text-muted-foreground mb-4">
              {String(executionError) || String(stepsError) || String(workflowError) || 'Unknown error occurred'}
            </p>
            <Button onClick={refetchAll}>
              <RotateCcw className="mr-2 h-4 w-4" />
              Retry
            </Button>
          </div>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center space-x-4">
        <Button variant="ghost" size="sm" asChild>
          <Link href="/runs">
            <ArrowLeft className="h-4 w-4" />
          </Link>
        </Button>
        <div className="flex-1">
          <div className="flex items-center space-x-3">
            {execution && getStatusIcon(execution.status, "h-6 w-6")}
            <h1 className="text-3xl font-bold tracking-tight">{flowName}</h1>
            {execution && getStatusBadge(execution.status)}
            {isDebugMode && (
              <Badge variant="outline" className="text-orange-600 border-orange-200">
                <Bug className="mr-1 h-3 w-3" />
                Debug Mode
              </Badge>
            )}
          </div>
          <p className="text-muted-foreground mt-1">
            Run ID: {executionId} • Started: {execution ? formatDate(execution.createdAt) : 'Loading...'}
          </p>
        </div>
        <div className="flex items-center space-x-2">
          {isDebugMode && (
            <>
              <Button variant="outline" size="sm">
                <Play className="mr-2 h-4 w-4" />
                Continue
              </Button>
              <Button variant="outline" size="sm">
                <Pause className="mr-2 h-4 w-4" />
                Pause
              </Button>
            </>
          )}
          <Button variant="outline" size="sm" onClick={refetchAll} disabled={executionLoading || stepsLoading || workflowLoading || flowDetailLoading}>
            <RotateCcw className={`mr-2 h-4 w-4 ${(executionLoading || stepsLoading || workflowLoading || flowDetailLoading) ? 'animate-spin' : ''}`} />
            Refresh
          </Button>
        </div>
      </div>

      {/* Overview Cards */}
      <div className="grid gap-4 md:grid-cols-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Progress</CardTitle>
            <Activity className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {executionLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                <div className="text-2xl font-bold">{completedSteps}/{totalSteps}</div>
                <Progress value={progress} className="mt-2" />
                <p className="text-xs text-muted-foreground mt-1">
                  {progress}% complete
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Duration</CardTitle>
            <Clock className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {executionLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : execution ? (
              <>
                <div className="text-2xl font-bold">{getDuration(execution.createdAt, execution.completedAt || undefined)}</div>
                <p className="text-xs text-muted-foreground">
                  {execution.status === 'running' ? 'Running time' : 'Total time'}
                </p>
              </>
            ) : null}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Completed</CardTitle>
            <CheckCircle className="h-4 w-4 text-green-500" />
          </CardHeader>
          <CardContent>
            {executionLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                <div className="text-2xl font-bold text-green-600">{completedSteps}</div>
                <p className="text-xs text-muted-foreground">
                  Steps finished
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Pending</CardTitle>
            <Clock className="h-4 w-4 text-yellow-500" />
          </CardHeader>
          <CardContent>
            {executionLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                <div className="text-2xl font-bold text-yellow-600">{pendingSteps}</div>
                <p className="text-xs text-muted-foreground">
                  Steps remaining
                </p>
              </>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Main Content Tabs */}
      <Tabs value={selectedTab} onValueChange={setSelectedTab}>
        <TabsList className="grid w-full grid-cols-3">
          <TabsTrigger value="overview">Overview & Output</TabsTrigger>
          <TabsTrigger value="steps">Steps</TabsTrigger>
          <TabsTrigger value="visualization">Flow Graph</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="space-y-4">
          <div className="grid gap-4 md:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Flow Information</CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                {workflowLoading ? (
                  <div className="flex items-center justify-center h-32">
                    <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                  </div>
                ) : (
                  <>
                    <div>
                      <span className="text-sm text-muted-foreground">Name:</span>
                      <div className="font-medium">{flowName}</div>
                    </div>
                    <div>
                      <span className="text-sm text-muted-foreground">Description:</span>
                      <div className="text-sm">{workflowData?.flow?.description || 'No description available'}</div>
                    </div>
                    {execution?.flowId && (
                      <div>
                        <span className="text-sm text-muted-foreground">Flow ID:</span>
                        <div className="font-mono text-sm">{execution.flowId.substring(0, 12)}...</div>
                      </div>
                    )}
                    <div>
                      <span className="text-sm text-muted-foreground">Total Steps:</span>
                      <div className="text-sm">{totalSteps}</div>
                    </div>
                  </>
                )}
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Run Output</CardTitle>
                <CardDescription>
                  Final output will appear here when the flow completes
                </CardDescription>
              </CardHeader>
              <CardContent>
                {executionLoading ? (
                  <div className="flex items-center justify-center h-32">
                    <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                  </div>
                ) : execution?.result ? (
                  <div className="bg-muted rounded-lg p-4">
                    <pre className="text-sm font-mono whitespace-pre-wrap">
                      {JSON.stringify(execution.result, null, 2)}
                    </pre>
                  </div>
                ) : (
                  <div className="flex items-center justify-center h-32 text-muted-foreground">
                    <div className="text-center">
                      <Clock className="h-8 w-8 mx-auto mb-2" />
                      <p>{execution?.status === 'running' ? 'Run in progress...' : 'No output available'}</p>
                    </div>
                  </div>
                )}
              </CardContent>
            </Card>
          </div>
        </TabsContent>

        <TabsContent value="steps" className="space-y-4">
          <Card>
            <CardHeader>
              <CardTitle>Run Steps</CardTitle>
              <CardDescription>
                Detailed view of each step in the flow execution
              </CardDescription>
            </CardHeader>
            <CardContent>
              <TooltipProvider>
                {stepsLoading ? (
                  <div className="flex items-center justify-center h-32">
                    <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                  </div>
                ) : steps?.length === 0 ? (
                  <div className="text-center py-8">
                    <Play className="w-8 h-8 text-gray-400 mx-auto mb-3" />
                    <h3 className="text-lg font-medium text-gray-900 mb-1">
                      No steps executed yet
                    </h3>
                    <p className="text-muted-foreground">
                      Steps will appear here as the flow executes
                    </p>
                  </div>
                ) : (
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>Step</TableHead>
                        <TableHead>Component</TableHead>
                        <TableHead>Status</TableHead>
                        <TableHead>Duration</TableHead>
                        <TableHead>Output Preview</TableHead>
                        <TableHead>Actions</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {visualizerSteps.map((step, index) => {
                        const stepExecution = steps?.find(s => s.stepIndex === index)
                        let status = 'pending'
                        let output = null

                        if (stepExecution?.result) {
                          const result = stepExecution.result;
                          if (typeof result === 'object' && 'Success' in result) {
                            status = 'completed'
                            output = JSON.stringify(result.Success)
                          } else if (typeof result === 'object' && 'Failed' in result) {
                            status = 'failed'
                            output = JSON.stringify(result.Failed)
                          } else if (typeof result === 'string') {
                            status = 'completed'
                            output = result
                          }
                        } else if (stepExecution?.state === 'running') {
                          status = 'running'
                        }

                        return (
                          <TableRow key={step.id}>
                            <TableCell>
                              <div className="flex items-center space-x-2">
                                {getStatusIcon(status)}
                                <span className="font-medium">{step.name}</span>
                              </div>
                            </TableCell>
                            <TableCell>
                              <Badge variant="outline">{step.component}</Badge>
                            </TableCell>
                            <TableCell>
                              {getStatusBadge(status)}
                            </TableCell>
                            <TableCell>
                              {step.duration || '-'}
                            </TableCell>
                            <TableCell>
                              {output ? (
                                <Tooltip>
                                  <TooltipTrigger asChild>
                                    <div className="font-mono text-xs max-w-40 truncate cursor-help">
                                      {output}
                                    </div>
                                  </TooltipTrigger>
                                  <TooltipContent side="left" className="max-w-80">
                                    <pre className="text-xs whitespace-pre-wrap">
                                      {output.length > 200
                                        ? `${output.substring(0, 200)}...`
                                        : output
                                      }
                                    </pre>
                                  </TooltipContent>
                                </Tooltip>
                              ) : (
                                <span className="text-muted-foreground">-</span>
                              )}
                            </TableCell>
                            <TableCell>
                              <div className="flex items-center space-x-1">
                                <StepOutputDialog step={{ ...step, output }} />
                                {isDebugMode && status === 'pending' && (
                                  <Button variant="ghost" size="sm">
                                    <Play className="h-4 w-4" />
                                  </Button>
                                )}
                              </div>
                            </TableCell>
                          </TableRow>
                        )
                      })}
                    </TableBody>
                  </Table>
                )}
              </TooltipProvider>
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="visualization">
          <Card>
            <CardHeader>
              <CardTitle>Flow Visualization</CardTitle>
              <CardDescription>
                Interactive graph showing the flow structure and execution progress
              </CardDescription>
            </CardHeader>
            <CardContent>
              <WorkflowVisualization
                visualizerSteps={visualizerSteps}
                dependencies={workflowDependencies}
                workflow={workflowData?.flow}
                analysisData={analysisData}
                isDebugMode={isDebugMode}
                isLoading={workflowLoading || stepsLoading || flowDetailLoading}
              />
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  )
}