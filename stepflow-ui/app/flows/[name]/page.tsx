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
  ListTree,
  History,
  Tags
} from 'lucide-react'
import { WorkflowExecutionDialog } from '@/components/workflow-execution-dialog'
import { WorkflowVisualizerBase } from '@/components/workflow-visualizer-base'
import {
  useFlow
} from '@/lib/hooks/use-flow-api'
import { extractStepDependencies } from '@/lib/dependency-types'

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

// WorkflowVisualization component removed - visualization tab not currently supported

export default function FlowDetailsPage() {
  const params = useParams()
  const router = useRouter()
  const searchParams = useSearchParams()
  const flowName = decodeURIComponent(params.name as string)

  // Get tab from URL or default to 'overview'
  const validTabs = ['overview', 'steps', 'visualization', 'runs'] as const
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
    router.replace(`/flows/${encodeURIComponent(flowName)}${newUrl}`, { scroll: false })
  }

  // API calls
  const { data: flowData, isLoading: flowLoading, error: flowError } = useFlow(flowName)
  
  // Recent executions are now included in the workflow data
  const flowRuns = flowData?.recentExecutions || []

  // Error handling
  if (flowError) {
    return (
      <div className="space-y-6">
        <div className="flex items-center gap-4">
          <Button variant="ghost" size="sm" asChild>
            <Link href="/flows">
              <ArrowLeft className="h-4 w-4" />
            </Link>
          </Button>
        </div>

        <div className="flex items-center justify-center h-64">
          <div className="text-center">
            <AlertCircle className="h-8 w-8 text-red-500 mx-auto mb-2" />
            <h2 className="text-lg font-semibold text-gray-900 mb-1">
              Failed to load flow
            </h2>
            <p className="text-muted-foreground mb-4">
              {flowError?.message || 'Unknown error occurred'}
            </p>
            <Button onClick={() => window.location.reload()}>
              Retry
            </Button>
          </div>
        </div>
      </div>
    )
  }

  const flow = flowData?.flow  // flow is now directly in flowData
  const totalSteps = flow?.steps?.length || 0

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
            <h1 className="text-3xl font-bold tracking-tight">{flowName}</h1>
            <Badge variant="outline">Flow</Badge>
          </div>
          <p className="text-muted-foreground mt-1">
            {flow?.description || 'No description available'} • {totalSteps} steps
          </p>
        </div>
        <div className="flex items-center space-x-2">
          <WorkflowExecutionDialog
            workflowName={flowName}
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
            {flowLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                <div className="text-2xl font-bold">{totalSteps}</div>
                <p className="text-xs text-muted-foreground">
                  Flow components
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Recent Runs</CardTitle>
            <History className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {flowLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                <div className="text-2xl font-bold">{flowRuns.length}</div>
                <p className="text-xs text-muted-foreground">
                  Last 10 runs
                </p>
              </>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">Labels</CardTitle>
            <Tags className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            {flowLoading ? (
              <div className="flex items-center justify-center h-20">
                <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <>
                <div className="text-2xl font-bold">{flowData?.labels?.length || 0}</div>
                <p className="text-xs text-muted-foreground">
                  Version labels
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
          <TabsTrigger value="visualization">Flow Graph</TabsTrigger>
          <TabsTrigger value="runs">Recent Runs</TabsTrigger>
        </TabsList>

        <TabsContent value="overview" className="space-y-4">
          {/* Analysis Status Card */}
          {flowData?.analysis && (
            <Card>
              <CardHeader>
                <CardTitle>Dependency Analysis</CardTitle>
                <CardDescription>
                  Flow analysis and validation results
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="grid gap-4 md:grid-cols-3">
                  <div className="text-center">
                    <div className="text-2xl font-bold text-blue-600">
                      {Object.keys(flowData.analysis.steps || {}).length}
                    </div>
                    <p className="text-xs text-muted-foreground">Steps Analyzed</p>
                  </div>
                  <div className="text-center">
                    <div className="text-2xl font-bold text-red-600">
                      {flowData.analysis.validationErrors?.length || 0}
                    </div>
                    <p className="text-xs text-muted-foreground">Validation Errors</p>
                  </div>
                  <div className="text-center">
                    <div className="text-2xl font-bold text-yellow-600">
                      {flowData.analysis.validationWarnings?.length || 0}
                    </div>
                    <p className="text-xs text-muted-foreground">Validation Warnings</p>
                  </div>
                </div>
                {(flowData.analysis.validationErrors?.length > 0 || flowData.analysis.validationWarnings?.length > 0) && (
                  <div className="mt-4 space-y-2">
                    {flowData.analysis.validationErrors?.map((error: unknown, index: number) => (
                      <div key={`error-${index}`} className="flex items-center text-sm text-red-600 bg-red-50 p-2 rounded">
                        <XCircle className="h-4 w-4 mr-2" />
                        {(error as { message?: string })?.message || 'Unknown error'}
                      </div>
                    ))}
                    {flowData.analysis.validationWarnings?.map((warning: unknown, index: number) => (
                      <div key={`warning-${index}`} className="flex items-center text-sm text-yellow-600 bg-yellow-50 p-2 rounded">
                        <AlertCircle className="h-4 w-4 mr-2" />
                        {(warning as { message?: string })?.message || 'Unknown warning'}
                      </div>
                    ))}
                  </div>
                )}
              </CardContent>
            </Card>
          )}
          
          <div className="grid gap-4 md:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle>Flow Information</CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                {flowLoading ? (
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
                      <div className="text-sm">{flow?.description || 'No description available'}</div>
                    </div>
                    {flowData?.flowId && (
                      <div>
                        <span className="text-sm text-muted-foreground">Flow ID:</span>
                        <div className="font-mono text-sm">{flowData.flowId.substring(0, 12)}...</div>
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
                  Expected input structure for this flow
                </CardDescription>
              </CardHeader>
              <CardContent>
                {flowLoading ? (
                  <div className="flex items-center justify-center h-32">
                    <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                  </div>
                ) : flow?.inputSchema ? (
                  <div className="bg-muted rounded-lg p-4">
                    <pre className="text-sm font-mono whitespace-pre-wrap">
                      {JSON.stringify(flow.inputSchema, null, 2)}
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
              <CardTitle>Flow Steps</CardTitle>
              <CardDescription>
                List of all steps in this flow
              </CardDescription>
            </CardHeader>
            <CardContent>
              <TooltipProvider>
                {flowLoading ? (
                  <div className="flex items-center justify-center h-32">
                    <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                  </div>
                ) : flow?.steps?.length === 0 ? (
                  <div className="text-center py-8">
                    <ListTree className="w-8 h-8 text-gray-400 mx-auto mb-3" />
                    <h3 className="text-lg font-medium text-gray-900 mb-1">
                      No steps defined
                    </h3>
                    <p className="text-muted-foreground">
                      This flow doesn&apos;t contain any steps
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
                      {flow?.steps?.map((step: { id: string; component: string; input?: Record<string, unknown>; depends_on?: string[] }) => (
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
                            {(() => {
                              // Use computed dependencies from analysis if available
                              const analysis = flowData?.analysis
                              if (analysis?.steps && analysis.steps[step.id]) {
                                const deps = extractStepDependencies(analysis.steps[step.id])
                                if (deps.length > 0) {
                                  return (
                                    <div className="flex flex-wrap gap-1">
                                      {deps.map((dep, depIndex) => (
                                        <Badge key={depIndex} variant="outline" className="text-xs">
                                          {dep}
                                        </Badge>
                                      ))}
                                    </div>
                                  )
                                }
                              }
                              
                              // Fallback to static dependencies from workflow definition
                              if (step.depends_on && step.depends_on.length > 0) {
                                return (
                                  <div className="flex flex-wrap gap-1">
                                    {step.depends_on.map((dep: string, depIndex: number) => (
                                      <Badge key={depIndex} variant="secondary" className="text-xs">
                                        {dep}
                                      </Badge>
                                    ))}
                                  </div>
                                )
                              }
                              
                              return <span className="text-muted-foreground">-</span>
                            })()}
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
              <CardTitle>Flow Visualization</CardTitle>
              <CardDescription>
                Interactive graph showing the flow structure and dependencies
              </CardDescription>
            </CardHeader>
            <CardContent>
              {flowLoading ? (
                <div className="flex items-center justify-center h-96">
                  <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
                </div>
              ) : !flow?.steps || flow.steps.length === 0 ? (
                <div className="flex items-center justify-center h-96 text-muted-foreground">
                  <div className="text-center">
                    <ListTree className="h-12 w-12 mx-auto mb-2" />
                    <p className="text-lg font-medium">No steps to visualize</p>
                    <p className="text-sm">This flow doesn&apos;t contain any steps</p>
                  </div>
                </div>
              ) : (
                <div className="h-96">
                  <WorkflowVisualizerBase
                    steps={flow.steps.map((step: { id: string; component: string }) => ({
                      id: step.id,
                      name: step.id,
                      component: step.component,
                      status: 'neutral' as const,
                      startTime: null,
                      duration: null,
                      output: null,
                    }))}
                    workflow={flow}
                    analysis={flowData?.analysis}
                    isDebugMode={false}
                    showExecutionData={false} // Static view, no execution data
                  />
                </div>
              )}
            </CardContent>
          </Card>
        </TabsContent>

        <TabsContent value="runs">
          <Card>
            <CardHeader>
              <CardTitle>Recent Runs</CardTitle>
              <CardDescription>
                Last 10 runs of this flow
              </CardDescription>
            </CardHeader>
            <CardContent>
              {flowLoading ? (
                <div className="flex items-center justify-center h-32">
                  <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
                </div>
              ) : flowRuns.length === 0 ? (
                <div className="text-center py-8">
                  <History className="w-8 h-8 text-gray-400 mx-auto mb-3" />
                  <h3 className="text-lg font-medium text-gray-900 mb-1">
                    No runs yet
                  </h3>
                  <p className="text-muted-foreground mb-4">
                    This flow hasn&apos;t been run yet
                  </p>
                  <WorkflowExecutionDialog
                    workflowName={flowName}
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
                      <TableHead>Run ID</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Started</TableHead>
                      <TableHead>Completed</TableHead>
                      <TableHead>Debug</TableHead>
                      <TableHead>Actions</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {flowRuns.map((execution) => (
                      <TableRow key={execution.id}>
                        <TableCell>
                          <div className="font-mono text-sm">
                            {execution.id.substring(0, 8)}...
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="flex items-center space-x-2">
                            {getStatusIcon(execution.status)}
                            {getStatusBadge(execution.status)}
                          </div>
                        </TableCell>
                        <TableCell>
                          <div className="text-sm">{formatDate(execution.createdAt)}</div>
                        </TableCell>
                        <TableCell>
                          {execution.completedAt ? (
                            <div className="text-sm">{formatDate(execution.completedAt)}</div>
                          ) : (
                            <span className="text-muted-foreground">-</span>
                          )}
                        </TableCell>
                        <TableCell>
                          {execution.debug && (
                            <Badge variant="outline" className="text-orange-600">
                              Debug
                            </Badge>
                          )}
                        </TableCell>
                        <TableCell>
                          <Button variant="ghost" size="sm" asChild>
                            <Link href={`/runs/${execution.id}`}>
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