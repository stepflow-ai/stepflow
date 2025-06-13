'use client'

import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Plus, Upload, FileText, Loader2 } from 'lucide-react'
import { useCreateEndpoint } from '@/lib/hooks/use-api'
import type { Workflow } from '@/lib/api'

interface CreateEndpointDialogProps {
  trigger?: React.ReactNode
}

const EXAMPLE_WORKFLOW = `{
  "input_schema": {
    "type": "object",
    "properties": {
      "message": {
        "type": "string",
        "description": "Message to process"
      }
    },
    "required": ["message"]
  },
  "steps": [
    {
      "id": "process_message",
      "component": "builtins://eval",
      "input": {
        "expression": "input.message + ' - processed'",
        "context": {
          "input": {
            "$from": {
              "workflow": "input"
            }
          }
        }
      }
    }
  ],
  "output": {
    "result": {
      "$from": {
        "step": "process_message"
      },
      "path": "result"
    }
  }
}`

export function CreateEndpointDialog({ trigger }: CreateEndpointDialogProps) {
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [label, setLabel] = useState('')
  const [workflowContent, setWorkflowContent] = useState(EXAMPLE_WORKFLOW)
  const [inputMethod, setInputMethod] = useState<'editor' | 'upload'>('editor')

  const createEndpointMutation = useCreateEndpoint()

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

      await createEndpointMutation.mutateAsync({
        name: name.trim(),
        data: { workflow },
        label: label.trim() || undefined
      })

      // Reset form and close dialog
      setName('')
      setLabel('')
      setWorkflowContent(EXAMPLE_WORKFLOW)
      setInputMethod('editor')
      setOpen(false)
    } catch (error) {
      console.error('Failed to create endpoint:', error)
    }
  }

  const loadExampleWorkflow = () => {
    setWorkflowContent(EXAMPLE_WORKFLOW)
  }

  const defaultTrigger = (
    <Button>
      <Plus className="mr-2 h-4 w-4" />
      Create Endpoint
    </Button>
  )

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        {trigger || defaultTrigger}
      </DialogTrigger>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <form onSubmit={handleSubmit}>
          <DialogHeader>
            <DialogTitle>Create New Endpoint</DialogTitle>
            <DialogDescription>
              Create a named endpoint for your workflow. You can optionally specify a label for versioning.
            </DialogDescription>
          </DialogHeader>

          <div className="grid gap-6 py-4">
            {/* Endpoint Details */}
            <div className="grid grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="endpoint-name">
                  Endpoint Name <span className="text-red-500">*</span>
                </Label>
                <Input
                  id="endpoint-name"
                  placeholder="my-workflow"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  required
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="endpoint-label">
                  Label (Optional)
                </Label>
                <Input
                  id="endpoint-label"
                  placeholder="v1.0, stable, beta..."
                  value={label}
                  onChange={(e) => setLabel(e.target.value)}
                />
                <p className="text-xs text-muted-foreground">
                  Leave empty to create the default version
                </p>
              </div>
            </div>

            {/* Workflow Definition */}
            <div className="space-y-4">
              <Label>Workflow Definition</Label>
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
                      <Label htmlFor="workflow-content">JSON Workflow Definition</Label>
                      <Button type="button" variant="outline" size="sm" onClick={loadExampleWorkflow}>
                        <FileText className="mr-2 h-3 w-3" />
                        Load Example
                      </Button>
                    </div>
                    <Textarea
                      id="workflow-content"
                      placeholder="Paste your workflow JSON here..."
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
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button 
              type="submit" 
              disabled={!name.trim() || !workflowContent.trim() || createEndpointMutation.isPending}
            >
              {createEndpointMutation.isPending ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Creating...
                </>
              ) : (
                <>
                  <Plus className="mr-2 h-4 w-4" />
                  Create Endpoint
                </>
              )}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}