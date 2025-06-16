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
import { useExecution, useExecutionSteps, useExecutionWorkflow, useWorkflowDependencies, transformStepsForVisualizer } from '@/lib/hooks/use-api'

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

function getStatusBadge(status: string) {
  const variants: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
    completed: 'default',
    running: 'secondary',
    failed: 'destructive',
    pending: 'outline',
  }

  return (
    <Badge variant={variants[status] || 'outline'}>
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
  dependencies?: Array<{
    step_index: number
    depends_on_step_index: number
    src_path?: string
    dst_field: { skip_if?: boolean; input?: boolean; input_field?: string }
    skip_action: { action: 'skip' | 'use_default'; default_value?: unknown }
  }>,
  workflow?: any,
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
      isDebugMode={isDebugMode}
      onStepClick={handleStepClick}
      onStepExecute={handleStepExecute}
    />
  )
}

export default function ExecutionDetailsPage() {
  const params = useParams()
  const router = useRouter()
  const searchParams = useSearchParams()
  const executionId = params.id as string

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
    router.replace(`/executions/${executionId}${newUrl}`, { scroll: false })
  }

  // API calls
  const { data: execution, isLoading: executionLoading, error: executionError, refetch: refetchExecution } = useExecution(executionId)
  const { data: steps, isLoading: stepsLoading, error: stepsError, refetch: refetchSteps } = useExecutionSteps(executionId)
  const { data: workflowData, isLoading: workflowLoading, error: workflowError, refetch: refetchWorkflow } = useExecutionWorkflow(executionId)
  const { data: workflowDependencies, isLoading: dependenciesLoading, error: dependenciesError, refetch: refetchDependencies } = useWorkflowDependencies(execution?.workflow_hash || workflowData?.workflow_hash || '')

  const refetchAll = () => {
    refetchExecution()
    refetchSteps()
    refetchWorkflow()
    refetchDependencies()
  }

  // Transform data for display
  const workflowName = execution?.workflow_name || workflowData?.workflow?.name || 'Ad-hoc Execution'
  const isDebugMode = execution?.debug_mode || false
  const totalSteps = workflowData?.workflow?.steps?.length || 0
  const completedSteps = steps?.filter(step =>
    step.result?.Success !== undefined ||
    step.result?.Failed !== undefined ||
    step.result?.Skipped !== undefined
  ).length || 0
  const runningSteps = steps?.filter(step =>
    step.started_at && !step.completed_at
  ).length || 0
  const pendingSteps = totalSteps - completedSteps - runningSteps
  const progress = totalSteps > 0 ? Math.round((completedSteps / totalSteps) * 100) : 0

  // Transform steps for visualizer
  const visualizerSteps = transformStepsForVisualizer(steps || [], workflowData?.workflow)

  // Error handling
  if (executionError || stepsError || workflowError || dependenciesError) {
    return (
      <div className="space-y-6">
        <div className="flex items-center gap-4">
          <Button variant="ghost" size="sm" asChild>
            <Link href="/executions">
              <ArrowLeft className="h-4 w-4" />
            </Link>
          </Button>
        </div>

        <div className="flex items-center justify-center h-64">
          <div className="text-center">
            <AlertCircle className="h-8 w-8 text-red-500 mx-auto mb-2" />
            <h2 className="text-lg font-semibold text-gray-900 mb-1">
              Failed to load execution
            </h2>
            <p className="text-muted-foreground mb-4">
              {executionError?.message || stepsError?.message || workflowError?.message || dependenciesError?.message || 'Unknown error occurred'}
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
          <Link href="/executions">
            <ArrowLeft className="h-4 w-4" />
          </Link>
        </Button>
        <div className="flex-1">
          <div className="flex items-center space-x-3">
            {execution && getStatusIcon(execution.status.toLowerCase(), "h-6 w-6")}
            <h1 className="text-3xl font-bold tracking-tight">{workflowName}</h1>
            {execution && getStatusBadge(execution.status.toLowerCase())}
            {isDebugMode && (
              <Badge variant="outline" className="text-orange-600 border-orange-200">
                <Bug className="mr-1 h-3 w-3" />
                Debug Mode
              </Badge>
            )}
          </div>
          <p className="text-muted-foreground mt-1">
            Execution ID: {executionId} • Started: {execution ? formatDate(execution.created_at) : 'Loading...'}
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
          <Button variant="outline" size="sm" onClick={refetchAll} disabled={executionLoading || stepsLoading || workflowLoading || dependenciesLoading}>
            <RotateCcw className={`mr-2 h-4 w-4 ${(executionLoading || stepsLoading || workflowLoading || dependenciesLoading) ? 'animate-spin' : ''}`} />
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
                <div className="text-2xl font-bold">{getDuration(execution.created_at, execution.completed_at)}</div>
                <p className="text-xs text-muted-foreground">
                  {execution.status === 'Running' ? 'Running time' : 'Total time'}
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
          <TabsTrigger value="visualization">Workflow Graph</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="space-y-4">
          <div className="grid gap-4 md:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Workflow Information</CardTitle>
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
                      <div className="font-medium">{workflowName}</div>
                    </div>
                    <div>
                      <span className="text-sm text-muted-foreground">Description:</span>
                      <div className="text-sm">{workflowData?.workflow?.description || 'No description available'}</div>
                    </div>
                    {execution?.workflow_hash && (
                      <div>
                        <span className="text-sm text-muted-foreground">Workflow Hash:</span>
                        <div className="font-mono text-sm">{execution.workflow_hash.substring(0, 12)}...</div>
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
                <CardTitle>Execution Output</CardTitle>
                <CardDescription>
                  Final output will appear here when the workflow completes
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
                      <p>{execution?.status === 'Running' ? 'Execution in progress...' : 'No output available'}</p>
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
              <CardTitle>Execution Steps</CardTitle>
              <CardDescription>
                Detailed view of each step in the workflow execution
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
                      Steps will appear here as the workflow executes
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
                        const stepExecution = steps?.find(s => s.step_index === index)
                        let status = 'pending'
                        let output = null

                        if (stepExecution?.result) {
                          if (stepExecution.result.Success !== undefined) {
                            status = 'completed'
                            output = JSON.stringify(stepExecution.result.Success)
                          } else if (stepExecution.result.Failed) {
                            status = 'failed'
                            output = JSON.stringify(stepExecution.result.Failed)
                          } else if (stepExecution.result.Skipped !== undefined) {
                            status = 'completed'
                            output = 'Skipped'
                          }
                        } else if (stepExecution?.started_at && !stepExecution?.completed_at) {
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
              <CardTitle>Workflow Visualization</CardTitle>
              <CardDescription>
                Interactive graph showing the workflow structure and execution progress
              </CardDescription>
            </CardHeader>
            <CardContent>
              <WorkflowVisualization
                visualizerSteps={visualizerSteps}
                dependencies={workflowDependencies?.dependencies}
                workflow={workflowData?.workflow}
                isDebugMode={isDebugMode}
                isLoading={workflowLoading || stepsLoading || dependenciesLoading}
              />
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  )
}