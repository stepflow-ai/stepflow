'use client'

import Link from 'next/link'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Activity, Clock, CheckCircle, XCircle, MoreHorizontal, Loader2 } from 'lucide-react'
import { useExecutions } from '@/lib/hooks/use-api'


function getStatusIcon(status: string) {
  switch (status.toLowerCase()) {
    case 'completed':
      return <CheckCircle className="h-4 w-4 text-green-500" />
    case 'running':
      return <Activity className="h-4 w-4 text-blue-500 animate-pulse" />
    case 'failed':
      return <XCircle className="h-4 w-4 text-red-500" />
    case 'pending':
      return <Clock className="h-4 w-4 text-yellow-500" />
    default:
      return <Clock className="h-4 w-4 text-gray-500" />
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
    <Badge variant={variants[status.toLowerCase()] || 'outline'}>
      {status}
    </Badge>
  )
}

export default function ExecutionsPage() {
  const { data: executions, isLoading, error, refetch } = useExecutions()

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="flex items-center space-x-2">
          <Loader2 className="h-6 w-6 animate-spin" />
          <span>Loading executions...</span>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center space-y-2">
          <p className="text-red-600">Failed to load executions</p>
          <p className="text-sm text-muted-foreground">{error.message}</p>
          <Button onClick={() => refetch()} variant="outline">
            <Activity className="mr-2 h-4 w-4" />
            Retry
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Executions</h1>
          <p className="text-muted-foreground">
            Monitor and manage workflow executions
          </p>
        </div>
        <Button onClick={() => refetch()}>
          <Activity className="mr-2 h-4 w-4" />
          Refresh
        </Button>
      </div>

      <div className="grid gap-4">
        {executions?.length === 0 ? (
          <Card>
            <CardContent className="flex items-center justify-center h-32">
              <div className="text-center space-y-2">
                <Activity className="h-8 w-8 text-muted-foreground mx-auto" />
                <p className="text-muted-foreground">No executions found</p>
                <p className="text-sm text-muted-foreground">Execute a workflow to see results here</p>
              </div>
            </CardContent>
          </Card>
        ) : (
          executions?.map((execution) => (
            <Card key={execution.executionId}>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <div className="flex items-center space-x-2">
                    {getStatusIcon(execution.status)}
                    <CardTitle className="text-lg">{execution.workflowName || 'Ad-hoc Workflow'}</CardTitle>
                    {getStatusBadge(execution.status)}
                  </div>
                  <div className="flex items-center space-x-2">
                    <Button variant="ghost" size="sm" asChild>
                      <Link href={`/executions/${execution.executionId}`}>
                        View Details
                      </Link>
                    </Button>
                    <Button variant="ghost" size="sm">
                      <MoreHorizontal className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
                <CardDescription>
                  Execution ID: {execution.executionId}
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                  <div>
                    <span className="text-muted-foreground">Started:</span>
                    <div>{new Date(execution.createdAt).toLocaleString()}</div>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Duration:</span>
                    <div>
                      {execution.completedAt ? (
                        `${Math.round((new Date(execution.completedAt).getTime() - new Date(execution.createdAt).getTime()) / 1000)}s`
                      ) : execution.status === 'running' ? (
                        `${Math.round((Date.now() - new Date(execution.createdAt).getTime()) / 1000)}s`
                      ) : (
                        '-'
                      )}
                    </div>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Debug Mode:</span>
                    <div>{execution.debugMode ? 'Yes' : 'No'}</div>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Status:</span>
                    <div className="capitalize">{execution.status}</div>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))
        )}
      </div>
    </div>
  )
}