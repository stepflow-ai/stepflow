'use client'

import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger, DropdownMenuItem, DropdownMenuSeparator } from '@/components/ui/dropdown-menu'
import { MoreVertical, Edit, Trash2, Copy, Eye, Loader2, AlertTriangle } from 'lucide-react'
import { useDeleteEndpoint } from '@/lib/hooks/use-api'
import { EditEndpointDialog } from '@/components/edit-endpoint-dialog'
import type { EndpointSummary } from '@/lib/api'

interface EndpointActionsMenuProps {
  endpoint: EndpointSummary
}

export function EndpointActionsMenu({ endpoint }: EndpointActionsMenuProps) {
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)
  const [editDialogOpen, setEditDialogOpen] = useState(false)
  const deleteEndpointMutation = useDeleteEndpoint()

  const handleDelete = async () => {
    try {
      await deleteEndpointMutation.mutateAsync({
        name: endpoint.name,
        label: endpoint.label
      })
      setDeleteDialogOpen(false)
    } catch (error) {
      console.error('Failed to delete endpoint:', error)
    }
  }

  const handleCopyEndpointUrl = () => {
    const baseUrl = process.env.STEPFLOW_API_URL || 'http://localhost:7837/api/v1'
    const url = endpoint.label 
      ? `${baseUrl}/endpoints/${endpoint.name}/execute?label=${endpoint.label}`
      : `${baseUrl}/endpoints/${endpoint.name}/execute`
    
    navigator.clipboard.writeText(url)
  }

  const handleViewWorkflow = () => {
    // TODO: Open workflow viewer dialog
    console.log('View workflow for', endpoint.name, endpoint.label)
  }

  const handleEditEndpoint = () => {
    setEditDialogOpen(true)
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button variant="ghost" size="sm">
            <MoreVertical className="h-4 w-4" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem onClick={handleViewWorkflow}>
            <Eye className="mr-2 h-4 w-4" />
            View Workflow
          </DropdownMenuItem>
          <DropdownMenuItem onClick={handleEditEndpoint}>
            <Edit className="mr-2 h-4 w-4" />
            Edit Endpoint
          </DropdownMenuItem>
          <DropdownMenuItem onClick={handleCopyEndpointUrl}>
            <Copy className="mr-2 h-4 w-4" />
            Copy API URL
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem 
            onClick={() => setDeleteDialogOpen(true)}
            className="text-red-600 focus:text-red-600"
          >
            <Trash2 className="mr-2 h-4 w-4" />
            Delete Endpoint
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Delete Confirmation Dialog */}
      <Dialog open={deleteDialogOpen} onOpenChange={setDeleteDialogOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center space-x-2">
              <AlertTriangle className="h-5 w-5 text-red-500" />
              <span>Delete Endpoint</span>
            </DialogTitle>
            <DialogDescription>
              Are you sure you want to delete the endpoint <strong>{endpoint.name}</strong>
              {endpoint.label && (
                <span> with label <strong>{endpoint.label}</strong></span>
              )}? This action cannot be undone.
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button 
              variant="outline" 
              onClick={() => setDeleteDialogOpen(false)}
              disabled={deleteEndpointMutation.isPending}
            >
              Cancel
            </Button>
            <Button 
              variant="destructive" 
              onClick={handleDelete}
              disabled={deleteEndpointMutation.isPending}
            >
              {deleteEndpointMutation.isPending ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Deleting...
                </>
              ) : (
                <>
                  <Trash2 className="mr-2 h-4 w-4" />
                  Delete
                </>
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Edit Endpoint Dialog */}
      <EditEndpointDialog 
        endpoint={endpoint}
        open={editDialogOpen}
        onOpenChange={setEditDialogOpen}
      />
    </>
  )
}