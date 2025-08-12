'use client'

import Link from 'next/link'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { FileText, Play, Loader2 } from 'lucide-react'
import { useFlows } from '@/lib/hooks/use-flow-api'
import { WorkflowExecutionDialog } from '@/components/workflow-execution-dialog'
import { FlowUploadDialog } from '@/components/flow-upload-dialog'
import { FlowLabelManagement } from '@/components/flow-label-management'

export default function FlowsPage() {
  const { data: flows, isLoading, error, refetch } = useFlows()

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="flex items-center space-x-2">
          <Loader2 className="h-6 w-6 animate-spin" />
          <span>Loading flows...</span>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center space-y-2">
          <p className="text-red-600">Failed to load flows</p>
          <p className="text-sm text-muted-foreground">{error.message}</p>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Flows</h1>
          <p className="text-muted-foreground">
            Manage flows by name and label for versioning
          </p>
        </div>
        <FlowUploadDialog
          trigger={
            <Button>
              Create Flow
            </Button>
          }
          onSuccess={() => refetch()}
        />
      </div>

      <div className="grid gap-4">
        {flows?.length === 0 ? (
          <Card>
            <CardContent className="flex items-center justify-center h-32">
              <div className="text-center space-y-2">
                <FileText className="h-8 w-8 text-muted-foreground mx-auto" />
                <p className="text-muted-foreground">No flows found</p>
                <p className="text-sm text-muted-foreground">Create your first flow to get started</p>
              </div>
            </CardContent>
          </Card>
        ) : (
          flows?.map((workflow) => (
          <Card key={workflow.id}>
            <CardHeader>
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-3">
                  <FileText className="h-5 w-5 text-muted-foreground" />
                  <div>
                    <CardTitle className="text-lg">{workflow.name}</CardTitle>
                    <CardDescription className="mt-1">
                      {workflow.description || 'No description available'}
                    </CardDescription>
                  </div>
                </div>
                <div className="flex items-center space-x-2">
                  <WorkflowExecutionDialog
                    workflowName={workflow.name}
                    trigger={
                      <Button variant="ghost" size="sm">
                        <Play className="mr-2 h-4 w-4" />
                        Execute Latest
                      </Button>
                    }
                  />
                  <FlowLabelManagement flowName={workflow.name} />
                  <Link href={`/run?flow=${encodeURIComponent(workflow.name)}`}>
                    <Button variant="outline" size="sm">
                      Full Page
                    </Button>
                  </Link>
                </div>
              </div>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 md:grid-cols-3 gap-4 text-sm">
                <div>
                  <span className="text-muted-foreground">Labels:</span>
                  <div className="flex flex-wrap gap-1 mt-1">
                    <Badge variant="outline" className="text-xs">
                      {workflow.labelCount} labels
                    </Badge>
                  </div>
                </div>
                <div>
                  <span className="text-muted-foreground">Latest Version:</span>
                  <div className="font-mono text-xs">{workflow.flowId.substring(0, 12)}...</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Last Updated:</span>
                  <div>{new Date(workflow.updatedAt).toLocaleDateString()}</div>
                </div>
              </div>
              
              <div className="mt-4 flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <Button variant="outline" size="sm" asChild>
                    <Link href={`/flows/${encodeURIComponent(workflow.name)}`}>
                      View Details
                    </Link>
                  </Button>
                  <Button variant="outline" size="sm" asChild>
                    <Link href={`/flows/${encodeURIComponent(workflow.name)}?tab=steps`}>
                      View Steps
                    </Link>
                  </Button>
                </div>
                <div className="text-sm text-muted-foreground">
                  {workflow.executionCount} runs
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