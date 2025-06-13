'use client'

import { useState, useEffect } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Edit, Upload, FileText, Loader2 } from 'lucide-react'
import { useCreateEndpoint, useEndpointWorkflow } from '@/lib/hooks/use-api'
import type { EndpointSummary, Workflow } from '@/lib/api'

interface EditEndpointDialogProps {
  endpoint: EndpointSummary
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function EditEndpointDialog({ endpoint, open, onOpenChange }: EditEndpointDialogProps) {
  const [name, setName] = useState(endpoint.name)
  const [label, setLabel] = useState(endpoint.label || '')
  const [workflowContent, setWorkflowContent] = useState('')
  const [inputMethod, setInputMethod] = useState<'editor' | 'upload'>('editor')

  const { data: workflowData, isLoading: workflowLoading } = useEndpointWorkflow(endpoint.name, endpoint.label)
  const createEndpointMutation = useCreateEndpoint()

  // Load existing workflow when dialog opens
  useEffect(() => {
    if (open && workflowData?.workflow) {
      setWorkflowContent(JSON.stringify(workflowData.workflow, null, 2))
    }
  }, [open, workflowData])

  // Reset form when dialog closes
  useEffect(() => {
    if (!open) {
      setName(endpoint.name)
      setLabel(endpoint.label || '')
      setWorkflowContent('')
      setInputMethod('editor')
    }
  }, [open, endpoint])

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    
    if (!name.trim()) {
      return
    }

    try {
      let workflow: Workflow
      try {
        workflow = JSON.parse(workflowContent)
      } catch (error) {
        console.error('Invalid workflow JSON:', error)
        return
      }

      // Create/update endpoint (PUT overwrites existing)
      await createEndpointMutation.mutateAsync({
        name: name.trim(),
        data: { workflow },
        label: label.trim() || undefined
      })

      onOpenChange(false)
    } catch (error) {
      console.error('Failed to update endpoint:', error)
    }
  }

  const loadExampleWorkflow = () => {
    const exampleWorkflow = {
      name: 'Updated Workflow',
      description: 'An updated workflow definition',
      input_schema: {
        type: 'object',
        properties: {
          message: {
            type: 'string',
            description: 'Message to process'
          }
        },
        required: ['message']
      },
      steps: [
        {
          id: 'process_message',
          component: 'builtins://eval',
          input: {
            expression: 'input.message + " - updated"',
            context: {
              input: {
                $from: {
                  workflow: 'input'
                }
              }
            }
          }
        }
      ],
      output: {
        result: {
          $from: {
            step: 'process_message'
          },
          path: 'result'
        }
      }
    }
    setWorkflowContent(JSON.stringify(exampleWorkflow, null, 2))
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle className="flex items-center space-x-2">
              <Edit className="h-5 w-5" />
              <span>Edit Endpoint</span>
            </DialogTitle>
            <DialogDescription>
              Update the endpoint configuration and workflow definition.
              {endpoint.label && (
                <span> Editing <strong>{endpoint.name}</strong> with label <strong>{endpoint.label}</strong>.</span>
              )}
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-6 py-4">
            {/* Endpoint Details */}
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="edit-endpoint-name">
                  Endpoint Name <span className="text-red-500">*</span>
                </Label>
                <Input
                  id="edit-endpoint-name"
                  placeholder="my-workflow"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                />
                <p className="text-xs text-muted-foreground">
                  Changing the name will create a new endpoint
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="edit-endpoint-label">
                  Label (Optional)
                </Label>
                <Input
                  id="edit-endpoint-label"
                  placeholder="v1.0, stable, beta..."
                  value={label}
                  onChange={(e) => setLabel(e.target.value)}
                />
                <p className="text-xs text-muted-foreground">
                  Leave empty for the default version
                </p>
              </div>
            </div>

            {/* Workflow Definition */}
            <div className="space-y-4">
              <Label>Workflow Definition</Label>
              {workflowLoading ? (
                <div className="flex items-center justify-center h-32">
                  <div className="flex items-center space-x-2">
                    <Loader2 className="h-6 w-6 animate-spin" />
                    <span>Loading current workflow...</span>
                  </div>
                </div>
              ) : (
                <Tabs value={inputMethod} onValueChange={(value) => setInputMethod(value as 'editor' | 'upload')}>
                  <TabsList className="grid w-full grid-cols-2">
                    <TabsTrigger value="editor">
                      <FileText className="mr-2 h-4 w-4" />
                      Editor
                    </TabsTrigger>
                    <TabsTrigger value="upload">
                      <Upload className="mr-2 h-4 w-4" />
                      Upload File
                    </TabsTrigger>
                  </TabsList>

                  <TabsContent value="editor" className="space-y-4">
                    <div className="space-y-2">
                      <div className="flex items-center justify-between">
                        <Label htmlFor="edit-workflow-content">JSON Workflow Definition</Label>
                        <Button type="button" variant="outline" size="sm" onClick={loadExampleWorkflow}>
                          <FileText className="mr-2 h-3 w-3" />
                          Load Example
                        </Button>
                      </div>
                      <Textarea
                        id="edit-workflow-content"
                        placeholder="Workflow JSON will be loaded here..."
                        className="min-h-[300px] font-mono text-sm"
                        value={workflowContent}
                        onChange={(e) => setWorkflowContent(e.target.value)}
                        required
                      />
                    </div>
                  </TabsContent>

                  <TabsContent value="upload" className="space-y-4">
                    <div className="border-2 border-dashed border-muted-foreground/25 rounded-lg p-8 text-center">
                      <Upload className="mx-auto h-12 w-12 text-muted-foreground/50" />
                      <p className="mt-2 text-sm text-muted-foreground">
                        Drag and drop a JSON file here, or click to browse
                      </p>
                      <Input
                        type="file"
                        accept=".json,.yaml,.yml"
                        className="mt-4"
                        onChange={(e) => {
                          const file = e.target.files?.[0]
                          if (file) {
                            const reader = new FileReader()
                            reader.onload = (event) => {
                              setWorkflowContent(event.target?.result as string)
                            }
                            reader.readAsText(file)
                          }
                        }}
                      />
                    </div>
                  </TabsContent>
                </Tabs>
              )}
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button 
              type="submit" 
              disabled={!name.trim() || !workflowContent.trim() || createEndpointMutation.isPending || workflowLoading}
            >
              {createEndpointMutation.isPending ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Updating...
                </>
              ) : (
                <>
                  <Edit className="mr-2 h-4 w-4" />
                  Update Endpoint
                </>
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}