'use client'

import Link from 'next/link'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Globe, Play, Loader2 } from 'lucide-react'
import { useEndpoints } from '@/lib/hooks/use-api'
import { CreateEndpointDialog } from '@/components/create-endpoint-dialog'
import { EndpointActionsMenu } from '@/components/endpoint-actions-menu'
import { ExecutionDialog } from '@/components/execution-dialog'


export default function EndpointsPage() {
  const { data: endpoints, isLoading, error } = useEndpoints()

  if (isLoading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="flex items-center space-x-2">
          <Loader2 className="h-6 w-6 animate-spin" />
          <span>Loading endpoints...</span>
        </div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-center space-y-2">
          <p className="text-red-600">Failed to load endpoints</p>
          <p className="text-sm text-muted-foreground">{error.message}</p>
        </div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Endpoints</h1>
          <p className="text-muted-foreground">
            Manage workflow endpoints and their versions
          </p>
        </div>
        <CreateEndpointDialog />
      </div>

      <div className="grid gap-4">
        {endpoints?.length === 0 ? (
          <Card>
            <CardContent className="flex items-center justify-center h-32">
              <div className="text-center space-y-2">
                <Globe className="h-8 w-8 text-muted-foreground mx-auto" />
                <p className="text-muted-foreground">No endpoints found</p>
                <p className="text-sm text-muted-foreground">Create your first endpoint to get started</p>
              </div>
            </CardContent>
          </Card>
        ) : (
          endpoints?.map((endpoint) => (
          <Card key={`${endpoint.name}-${endpoint.label || 'default'}`}>
            <CardHeader>
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-3">
                  <Globe className="h-5 w-5 text-muted-foreground" />
                  <div>
                    <CardTitle className="text-lg">{endpoint.name}</CardTitle>
                    <CardDescription className="mt-1">
                      {endpoint.label ? `Endpoint with label: ${endpoint.label}` : 'Default endpoint'}
                    </CardDescription>
                  </div>
                  {endpoint.label && (
                    <Badge variant="outline">{endpoint.label}</Badge>
                  )}
                </div>
                <div className="flex items-center space-x-2">
                  <ExecutionDialog
                    endpoint={endpoint}
                    trigger={
                      <Button variant="ghost" size="sm">
                        <Play className="mr-2 h-4 w-4" />
                        Execute
                      </Button>
                    }
                  />
                  <Link href={`/execute?endpoint=${endpoint.name}${endpoint.label ? `&label=${endpoint.label}` : ''}`}>
                    <Button variant="outline" size="sm">
                      Full Page
                    </Button>
                  </Link>
                  <EndpointActionsMenu endpoint={endpoint} />
                </div>
              </div>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 md:grid-cols-3 gap-4 text-sm">
                <div>
                  <span className="text-muted-foreground">Workflow Hash:</span>
                  <div className="font-mono text-xs">{endpoint.workflow_hash?.substring(0, 12)}...</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Created:</span>
                  <div>{new Date(endpoint.created_at).toLocaleDateString()}</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Updated:</span>
                  <div>{new Date(endpoint.updated_at).toLocaleDateString()}</div>
                </div>
              </div>
              
              <div className="mt-4 flex items-center justify-between">
                <div className="flex items-center space-x-2">
                  <Button variant="outline" size="sm">
                    View Details
                  </Button>
                  <Button variant="outline" size="sm">
                    API Docs
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