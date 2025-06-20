'use client'

import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Badge } from '@/components/ui/badge'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Tags, Plus, Trash2, Loader2, RotateCcw } from 'lucide-react'
import { useFlowLabels, useCreateFlowLabel, useDeleteFlowLabel, useFlow } from '@/lib/hooks/use-flow-api'
import { toast } from 'sonner'

interface FlowLabelManagementProps {
  flowName: string
  trigger?: React.ReactNode
}

export function FlowLabelManagement({ flowName, trigger }: FlowLabelManagementProps) {
  const [open, setOpen] = useState(false)
  const [newLabelName, setNewLabelName] = useState('')
  const [selectedFlowHash, setSelectedFlowHash] = useState('')

  // API hooks
  const { data: labels, isLoading: labelsLoading, refetch: refetchLabels } = useFlowLabels(flowName)
  const { data: flow, isLoading: flowLoading } = useFlow(flowName)
  const createLabelMutation = useCreateFlowLabel()
  const deleteLabelMutation = useDeleteFlowLabel()

  // Get available flow versions (current flow + versions referenced by existing labels)
  const getAvailableVersions = () => {
    const versions: Array<{
      flowHash: string
      description: string
      createdAt: string
    }> = []
    
    // Add current flow version
    if (flow?.flowHash) {
      versions.push({
        flowHash: flow.flowHash,
        description: 'Current version',
        createdAt: flow.updatedAt,
      })
    }
    
    // Add versions from existing labels (if different from current)
    labels?.forEach(label => {
      if (label.flowHash !== flow?.flowHash && !versions.find(v => v.flowHash === label.flowHash)) {
        versions.push({
          flowHash: label.flowHash,
          description: `Referenced by "${label.label}"`,
          createdAt: label.createdAt,
        })
      }
    })
    
    return versions
  }

  const availableVersions = getAvailableVersions()

  const handleCreateLabel = async (e: React.FormEvent) => {
    e.preventDefault()
    
    if (!newLabelName.trim()) {
      toast.error('Label name is required')
      return
    }
    
    if (!selectedFlowHash) {
      toast.error('Please select a flow version')
      return
    }

    try {
      await createLabelMutation.mutateAsync({
        name: flowName,
        label: newLabelName.trim(),
        flowHash: selectedFlowHash,
      })

      toast.success(`Label "${newLabelName}" created successfully!`)
      setNewLabelName('')
      setSelectedFlowHash('')
    } catch (error) {
      console.error('Failed to create label:', error)
      toast.error(error instanceof Error ? error.message : 'Failed to create label')
    }
  }

  const handleDeleteLabel = async (labelName: string) => {
    if (!confirm(`Are you sure you want to delete the label "${labelName}"?`)) {
      return
    }

    try {
      await deleteLabelMutation.mutateAsync({
        name: flowName,
        label: labelName,
      })

      toast.success(`Label "${labelName}" deleted successfully!`)
    } catch (error) {
      console.error('Failed to delete label:', error)
      toast.error(error instanceof Error ? error.message : 'Failed to delete label')
    }
  }

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    })
  }

  const defaultTrigger = (
    <Button variant="outline" size="sm">
      <Tags className="mr-2 h-4 w-4" />
      Manage Labels
    </Button>
  )

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        {trigger || defaultTrigger}
      </DialogTrigger>
      <DialogContent className="max-w-4xl max-h-[80vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center space-x-2">
            <Tags className="h-5 w-5" />
            <span>Manage Labels for &ldquo;{flowName}&rdquo;</span>
          </DialogTitle>
          <DialogDescription>
            Create and manage version labels for this flow. Labels allow you to mark specific versions 
            as &ldquo;production&rdquo;, &ldquo;staging&rdquo;, etc. for easy reference.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          {/* Create New Label Section */}
          <div className="border rounded-lg p-4">
            <h3 className="font-semibold mb-4">Create New Label</h3>
            <form onSubmit={handleCreateLabel} className="space-y-4">
              <div className="grid grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="label-name">Label Name</Label>
                  <Input
                    id="label-name"
                    value={newLabelName}
                    onChange={(e) => setNewLabelName(e.target.value)}
                    placeholder="production, staging, v1.0, etc."
                    disabled={createLabelMutation.isPending}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="flow-version">Flow Version</Label>
                  <Select 
                    value={selectedFlowHash} 
                    onValueChange={setSelectedFlowHash}
                    disabled={flowLoading || createLabelMutation.isPending}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder="Select a version" />
                    </SelectTrigger>
                    <SelectContent>
                      {flowLoading ? (
                        <SelectItem value="loading" disabled>
                          <div className="flex items-center space-x-2">
                            <Loader2 className="h-4 w-4 animate-spin" />
                            <span>Loading versions...</span>
                          </div>
                        </SelectItem>
                      ) : availableVersions.length === 0 ? (
                        <SelectItem value="no-versions" disabled>
                          No versions available
                        </SelectItem>
                      ) : (
                        availableVersions.map((version) => (
                          <SelectItem key={version.flowHash} value={version.flowHash}>
                            <div className="flex flex-col">
                              <div className="flex items-center justify-between w-full">
                                <span className="font-mono text-sm">
                                  {version.flowHash.substring(0, 12)}...
                                </span>
                                <span className="text-xs text-muted-foreground ml-2">
                                  {formatDate(version.createdAt)}
                                </span>
                              </div>
                              <span className="text-xs text-muted-foreground text-left">
                                {version.description}
                              </span>
                            </div>
                          </SelectItem>
                        ))
                      )}
                    </SelectContent>
                  </Select>
                </div>
              </div>
              <Button
                type="submit"
                disabled={createLabelMutation.isPending || !newLabelName.trim() || !selectedFlowHash}
                size="sm"
              >
                {createLabelMutation.isPending ? (
                  <>
                    <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                    Creating...
                  </>
                ) : (
                  <>
                    <Plus className="mr-2 h-4 w-4" />
                    Create Label
                  </>
                )}
              </Button>
            </form>
          </div>

          {/* Existing Labels Section */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="font-semibold">Existing Labels</h3>
              <Button variant="outline" size="sm" onClick={() => refetchLabels()}>
                <RotateCcw className="mr-2 h-4 w-4" />
                Refresh
              </Button>
            </div>

            {labelsLoading ? (
              <div className="flex items-center justify-center py-8">
                <div className="flex items-center space-x-2">
                  <Loader2 className="h-5 w-5 animate-spin" />
                  <span>Loading labels...</span>
                </div>
              </div>
            ) : labels?.length === 0 ? (
              <div className="text-center py-8 text-muted-foreground">
                <Tags className="h-8 w-8 mx-auto mb-2 opacity-50" />
                <p>No labels found for this flow</p>
                <p className="text-sm">Create a label above to get started</p>
              </div>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Label</TableHead>
                    <TableHead>Flow Version</TableHead>
                    <TableHead>Created</TableHead>
                    <TableHead>Updated</TableHead>
                    <TableHead className="w-[100px]">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {labels?.map((label) => (
                    <TableRow key={label.label}>
                      <TableCell>
                        <Badge variant="outline" className="font-mono">
                          {label.label}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <code className="text-sm bg-muted px-2 py-1 rounded">
                          {label.flowHash.substring(0, 12)}...
                        </code>
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {formatDate(label.createdAt)}
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {formatDate(label.updatedAt)}
                      </TableCell>
                      <TableCell>
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => handleDeleteLabel(label.label)}
                          disabled={deleteLabelMutation.isPending}
                        >
                          {deleteLabelMutation.isPending ? (
                            <Loader2 className="h-4 w-4 animate-spin" />
                          ) : (
                            <Trash2 className="h-4 w-4" />
                          )}
                        </Button>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)}>
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}