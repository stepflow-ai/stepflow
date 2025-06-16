'use client'

import Link from 'next/link'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { FileText, Play, Loader2, Tags } from 'lucide-react'
import { useWorkflowNames } from '@/lib/hooks/use-api'
import { WorkflowExecutionDialog } from '@/components/workflow-execution-dialog'

export default function WorkflowsPage() {
  const { data: workflowNames, isLoading, error } = useWorkflowNames()

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="flex items-center space-x-2">
          <Loader2 className="h-6 w-6 animate-spin" />
          <span>Loading workflows...</span>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center space-y-2">
          <p className="text-red-600">Failed to load workflows</p>
          <p className="text-sm text-muted-foreground">{error.message}</p>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Workflows</h1>
          <p className="text-muted-foreground">
            Manage workflows by name and label for versioning
          </p>
        </div>
        <Button>
          Upload Workflow
        </Button>
      </div>

      <div className="grid gap-4">
        {workflowNames?.length === 0 ? (
          <Card>
            <CardContent className="flex items-center justify-center h-32">
              <div className="text-center space-y-2">
                <FileText className="h-8 w-8 text-muted-foreground mx-auto" />
                <p className="text-muted-foreground">No workflows found</p>
                <p className="text-sm text-muted-foreground">Upload or create your first workflow to get started</p>
              </div>
            </CardContent>
          </Card>
        ) : (
          workflowNames?.map((name) => (
          <Card key={name}>
            <CardHeader>
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-3">
                  <FileText className="h-5 w-5 text-muted-foreground" />
                  <div>
                    <CardTitle className="text-lg">{name}</CardTitle>
                    <CardDescription className="mt-1">
                      Workflow with multiple versions/labels
                    </CardDescription>
                  </div>
                </div>
                <div className="flex items-center space-x-2">
                  <WorkflowExecutionDialog
                    workflowName={name}
                    trigger={
                      <Button variant="ghost" size="sm">
                        <Play className="mr-2 h-4 w-4" />
                        Execute Latest
                      </Button>
                    }
                  />
                  <Button variant="outline" size="sm">
                    <Tags className="mr-2 h-4 w-4" />
                    Manage Labels
                  </Button>
                  <Link href={`/execute?workflow=${encodeURIComponent(name)}`}>
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
                    <Badge variant="outline" className="text-xs">production</Badge>
                    <Badge variant="outline" className="text-xs">staging</Badge>
                    <Badge variant="outline" className="text-xs">latest</Badge>
                  </div>
                </div>
                <div>
                  <span className="text-muted-foreground">Latest Version:</span>
                  <div className="font-mono text-xs">abc123...def</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Last Updated:</span>
                  <div>2 hours ago</div>
                </div>
              </div>
              
              <div className="mt-4 flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <Button variant="outline" size="sm" asChild>
                    <Link href={`/workflows/${encodeURIComponent(name)}`}>
                      View Details
                    </Link>
                  </Button>
                  <Button variant="outline" size="sm" asChild>
                    <Link href={`/workflows/${encodeURIComponent(name)}?tab=visualization`}>
                      Dependencies
                    </Link>
                  </Button>
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