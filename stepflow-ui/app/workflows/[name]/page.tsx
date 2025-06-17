'use client'

import Link from 'next/link'
import { useParams, useRouter, useSearchParams } from 'next/navigation'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { TooltipProvider } from '@/components/ui/tooltip'
import {
  ArrowLeft,
  Activity,
  CheckCircle,
  XCircle,
  Clock,
  Play,
  Loader2,
  AlertCircle,
  Code,
  Network,
  ListTree,
  History
} from 'lucide-react'
import { WorkflowVisualizerBase } from '@/components/workflow-visualizer-base'
import { WorkflowExecutionDialog } from '@/components/workflow-execution-dialog'
import {
  useLatestWorkflowByName,
  useWorkflowDependencies,
  useExecutions
} from '@/lib/hooks/use-api'
import type { StepDependency } from '@/api-client'

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

function getStatusIcon(status: string, className = "h-4 w-4") {
  switch (status) {
    case 'Completed':
      return <CheckCircle className={`${className} text-green-500`} />
    case 'Running':
      return <Activity className={`${className} text-blue-500 animate-pulse`} />
    case 'Failed':
      return <XCircle className={`${className} text-red-500`} />
    case 'Pending':
      return <Clock className={`${className} text-yellow-500`} />
    default:
      return <Clock className={`${className} text-gray-500`} />
  }
}

function getStatusBadge(status: string) {
  const variants: Record<string, 'default' | 'secondary' | 'destructive' | 'outline'> = {
    Completed: 'default',
    Running: 'secondary',
    Failed: 'destructive',
    Pending: 'outline',
  }

  return (
    <Badge variant={variants[status] || 'outline'}>
      {status}
    </Badge>
  )
}

function WorkflowVisualization({
  workflow,
  dependencies,
  isLoading
}: {
  workflow?: any,
  dependencies?: StepDependency[],
  isLoading: boolean
}) {
  const handleStepClick = (stepId: string) => {
    console.log('Opening step details for:', stepId)
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-96">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    )
  }

  if (!workflow?.steps || workflow.steps.length === 0) {
    return (
      <div className="flex items-center justify-center h-96 text-muted-foreground">
        <div className="text-center">
          <Network className="h-12 w-12 mx-auto mb-2" />
          <p className="text-lg font-medium">No workflow steps available</p>
          <p className="text-sm">Workflow structure will appear here when available</p>
        </div>
      </div>
    )
  }

  // Transform workflow steps for visualization (without execution data)
  const visualizerSteps = workflow.steps.map((step: any, index: number) => ({
    id: step.id,
    name: step.id,
    component: step.component,
    status: 'neutral' as const, // Use neutral status for workflow view
    startTime: null,
    duration: null,
    output: null
  }))

  return (
    <WorkflowVisualizerBase
      steps={visualizerSteps}
      dependencies={dependencies}
      workflow={workflow}
      isDebugMode={false}
      showExecutionData={false} // Workflow view doesn't show execution data
      onStepClick={handleStepClick}
    />
  )
}

export default function WorkflowDetailsPage() {
  const params = useParams()
  const router = useRouter()
  const searchParams = useSearchParams()
  const workflowName = decodeURIComponent(params.name as string)

  // Get tab from URL or default to 'overview'
  const validTabs = ['overview', 'steps', 'visualization', 'executions'] as const
  const tabFromUrl = searchParams.get('tab')
  const selectedTab: string = tabFromUrl && validTabs.includes(tabFromUrl as typeof validTabs[number])
    ? tabFromUrl
    : 'overview'

  // Function to update tab in URL
  const setSelectedTab = (tab: string) => {
    const newParams = new URLSearchParams(searchParams.toString())
    if (tab === 'overview') {
      newParams.delete('tab')
    } else {
      newParams.set('tab', tab)
    }
    const newUrl = newParams.toString() ? `?${newParams.toString()}` : ''
    router.replace(`/workflows/${encodeURIComponent(workflowName)}${newUrl}`, { scroll: false })
  }

  // API calls
  const { data: workflowData, isLoading: workflowLoading, error: workflowError } = useLatestWorkflowByName(workflowName)
  const { data: workflowDependencies, isLoading: dependenciesLoading, error: dependenciesError } = useWorkflowDependencies(workflowData?.workflowHash || '')
  const { data: executionsData, isLoading: executionsLoading, error: executionsError } = useExecutions()

  // Filter executions for this workflow
  const workflowExecutions = executionsData?.filter(execution =>
    execution.workflowName === workflowName
  ).slice(0, 10) || [] // Show last 10 executions

  // Error handling
  if (workflowError || dependenciesError) {
    return (
      <div className="space-y-6">
        <div className="flex items-center gap-4">
          <Button variant="ghost" size="sm" asChild>
            <Link href="/workflows">
              <ArrowLeft className="h-4 w-4" />
            </Link>
          </Button>
        </div>

        <div className="flex items-center justify-center h-64">
          <div className="text-center">
            <AlertCircle className="h-8 w-8 text-red-500 mx-auto mb-2" />
            <h2 className="text-lg font-semibold text-gray-900 mb-1">
              Failed to load workflow
            </h2>
            <p className="text-muted-foreground mb-4">
              {workflowError?.message || dependenciesError?.message || 'Unknown error occurred'}
            </p>
            <Button onClick={() => window.location.reload()}>
              Retry
            </Button>
          </div>
        </div>
      </div>
    )
  }

  const workflow = workflowData?.workflow
  const totalSteps = workflow?.steps?.length || 0

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center space-x-4">
        <Button variant="ghost" size="sm" asChild>
          <Link href="/workflows">
            <ArrowLeft className="h-4 w-4" />
          </Link>
        </Button>
        <div className="flex-1">
          <div className="flex items-center space-x-3">
            <Code className="h-6 w-6 text-blue-600" />
            <h1 className="text-3xl font-bold tracking-tight">{workflowName}</h1>
            <Badge variant="outline">Workflow</Badge>
          </div>
          <p className="text-muted-foreground mt-1">
            {workflow?.description || 'No description available'} • {totalSteps} steps
          </p>
        </div>
        <div className="flex items-center space-x-2">
          <WorkflowExecutionDialog
            workflowName={workflowName}
            trigger={
              <Button>
                <Play className="mr-2 h-4 w-4" />
                Execute
              </Button>
            }
          />
        </div>
      </div>

      {/* Overview Cards */}
      <div className="grid gap-4 md:grid-cols-3">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Total Steps</CardTitle>
            <ListTree className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {workflowLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                <div className="text-2xl font-bold">{totalSteps}</div>
                <p className="text-xs text-muted-foreground">
                  Workflow components
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Recent Executions</CardTitle>
            <History className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {executionsLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                <div className="text-2xl font-bold">{workflowExecutions.length}</div>
                <p className="text-xs text-muted-foreground">
                  Last 10 executions
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Dependencies</CardTitle>
            <Network className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {dependenciesLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                <div className="text-2xl font-bold">{workflowDependencies?.dependencies?.length || 0}</div>
                <p className="text-xs text-muted-foreground">
                  Step dependencies
                </p>
              </>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Main Content Tabs */}
      <Tabs value={selectedTab} onValueChange={setSelectedTab}>
        <TabsList className="grid w-full grid-cols-4">
          <TabsTrigger value="overview">Overview</TabsTrigger>
          <TabsTrigger value="steps">Steps</TabsTrigger>
          <TabsTrigger value="visualization">Workflow Graph</TabsTrigger>
          <TabsTrigger value="executions">Recent Executions</TabsTrigger>
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
                      <div className="text-sm">{workflow?.description || 'No description available'}</div>
                    </div>
                    {workflowData?.workflowHash && (
                      <div>
                        <span className="text-sm text-muted-foreground">Workflow Hash:</span>
                        <div className="font-mono text-sm">{workflowData.workflowHash.substring(0, 12)}...</div>
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
                <CardTitle>Input Schema</CardTitle>
                <CardDescription>
                  Expected input structure for this workflow
                </CardDescription>
              </CardHeader>
              <CardContent>
                {workflowLoading ? (
                  <div className="flex items-center justify-center h-32">
                    <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                  </div>
                ) : workflow?.inputSchema ? (
                  <div className="bg-muted rounded-lg p-4">
                    <pre className="text-sm font-mono whitespace-pre-wrap">
                      {JSON.stringify(workflow.inputSchema, null, 2)}
                    </pre>
                  </div>
                ) : (
                  <div className="flex items-center justify-center h-32 text-muted-foreground">
                    <div className="text-center">
                      <Code className="h-8 w-8 mx-auto mb-2" />
                      <p>No input schema defined</p>
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
              <CardTitle>Workflow Steps</CardTitle>
              <CardDescription>
                List of all steps in this workflow
              </CardDescription>
            </CardHeader>
            <CardContent>
              <TooltipProvider>
                {workflowLoading ? (
                  <div className="flex items-center justify-center h-32">
                    <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                  </div>
                ) : workflow?.steps?.length === 0 ? (
                  <div className="text-center py-8">
                    <ListTree className="w-8 h-8 text-gray-400 mx-auto mb-3" />
                    <h3 className="text-lg font-medium text-gray-900 mb-1">
                      No steps defined
                    </h3>
                    <p className="text-muted-foreground">
                      This workflow doesn't contain any steps
                    </p>
                  </div>
                ) : (
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>Step ID</TableHead>
                        <TableHead>Component</TableHead>
                        <TableHead>Input Fields</TableHead>
                        <TableHead>Dependencies</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {workflow?.steps?.map((step: any, index: number) => (
                        <TableRow key={step.id}>
                          <TableCell>
                            <div className="font-medium">{step.id}</div>
                          </TableCell>
                          <TableCell>
                            <Badge variant="outline">{step.component}</Badge>
                          </TableCell>
                          <TableCell>
                            {step.input && typeof step.input === 'object' ? (
                              <div className="flex flex-wrap gap-1">
                                {Object.keys(step.input).map((fieldName) => (
                                  <Badge key={fieldName} variant="secondary" className="text-xs">
                                    {fieldName}
                                  </Badge>
                                ))}
                              </div>
                            ) : (
                              <span className="text-muted-foreground">-</span>
                            )}
                          </TableCell>
                          <TableCell>
                            {step.depends_on && step.depends_on.length > 0 ? (
                              <div className="flex flex-wrap gap-1">
                                {step.depends_on.map((dep: string, depIndex: number) => (
                                  <Badge key={depIndex} variant="outline" className="text-xs">
                                    {dep}
                                  </Badge>
                                ))}
                              </div>
                            ) : (
                              <span className="text-muted-foreground">-</span>
                            )}
                          </TableCell>
                        </TableRow>
                      ))}
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
              <CardTitle>Workflow Graph</CardTitle>
              <CardDescription>
                Visual representation of the workflow structure and dependencies
              </CardDescription>
            </CardHeader>
            <CardContent>
              <WorkflowVisualization
                workflow={workflow}
                dependencies={workflowDependencies?.dependencies}
                isLoading={workflowLoading || dependenciesLoading}
              />
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="executions">
          <Card>
            <CardHeader>
              <CardTitle>Recent Executions</CardTitle>
              <CardDescription>
                Last 10 executions of this workflow
              </CardDescription>
            </CardHeader>
            <CardContent>
              {executionsLoading ? (
                <div className="flex items-center justify-center h-32">
                  <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                </div>
              ) : workflowExecutions.length === 0 ? (
                <div className="text-center py-8">
                  <History className="w-8 h-8 text-gray-400 mx-auto mb-3" />
                  <h3 className="text-lg font-medium text-gray-900 mb-1">
                    No executions yet
                  </h3>
                  <p className="text-muted-foreground mb-4">
                    This workflow hasn't been executed yet
                  </p>
                  <WorkflowExecutionDialog
                    workflowName={workflowName}
                    trigger={
                      <Button>
                        <Play className="mr-2 h-4 w-4" />
                        Execute Now
                      </Button>
                    }
                  />
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Execution ID</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Started</TableHead>
                      <TableHead>Completed</TableHead>
                      <TableHead>Debug</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {workflowExecutions.map((execution) => (
                      <TableRow key={execution.executionId}>
                        <TableCell>
                          <div className="font-mono text-sm">
                            {execution.executionId.substring(0, 8)}...
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="flex items-center space-x-2">
                            {getStatusIcon(execution.status)}
                            {getStatusBadge(execution.status)}
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="text-sm">{formatDate(execution.createdAt.toString())}</div>
                        </TableCell>
                        <TableCell>
                          {execution.completedAt ? (
                            <div className="text-sm">{formatDate(execution.completedAt.toString())}</div>
                          ) : (
                            <span className="text-muted-foreground">-</span>
                          )}
                        </TableCell>
                        <TableCell>
                          {execution.debugMode && (
                            <Badge variant="outline" className="text-orange-600">
                              Debug
                            </Badge>
                          )}
                        </TableCell>
                        <TableCell>
                          <Button variant="ghost" size="sm" asChild>
                            <Link href={`/executions/${execution.executionId}`}>
                              View
                            </Link>
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  )
}