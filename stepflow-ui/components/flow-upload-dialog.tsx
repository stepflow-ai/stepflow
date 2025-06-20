'use client'

import { useState, useCallback, useEffect } from 'react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { Textarea } from '@/components/ui/textarea'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from '@/components/ui/dialog'
import { Plus, FileText, Loader2, Code, Lightbulb, Upload } from 'lucide-react'
import { useStoreFlow } from '@/lib/hooks/use-flow-api'
import { toast } from 'sonner'
import { type InputExample } from '@/lib/examples'

interface FlowUploadDialogProps {
  trigger: React.ReactNode
  onSuccess?: () => void
}

// Common workflow templates
const WORKFLOW_EXAMPLES: InputExample[] = [
  {
    name: 'Simple Math',
    description: 'Basic arithmetic workflow',
    format: 'json',
    data: {
      name: 'simple-math',
      description: 'A simple arithmetic workflow',
      input_schema: {
        type: 'object',
        properties: {
          a: { type: 'integer' },
          b: { type: 'integer' }
        }
      },
      steps: [
        {
          id: 'add',
          component: 'python://add',
          input: {
            a: { $from: { workflow: 'input' }, path: 'a' },
            b: { $from: { workflow: 'input' }, path: 'b' }
          }
        }
      ],
      output: {
        result: { $from: { step: 'add' }, path: 'result' }
      }
    }
  },
  {
    name: 'Data Processing',
    description: 'Process data through multiple steps',
    format: 'json',
    data: {
      name: 'data-processing',
      description: 'Multi-step data processing workflow',
      input_schema: {
        type: 'object',
        properties: {
          file_path: { type: 'string' }
        }
      },
      steps: [
        {
          id: 'load',
          component: 'builtins://load_file',
          input: {
            path: { $from: { workflow: 'input' }, path: 'file_path' }
          }
        },
        {
          id: 'multiply',
          component: 'python://multiply',
          input: {
            a: { $from: { step: 'load' }, path: 'result' },
            b: 2
          }
        }
      ],
      output: {
        processed_data: { $from: { step: 'multiply' }, path: 'result' }
      }
    }
  },
  {
    name: 'OpenAI Chat',
    description: 'Simple AI chat workflow',
    format: 'json',
    data: {
      name: 'openai-chat',
      description: 'OpenAI chat completion workflow',
      input_schema: {
        type: 'object',
        properties: {
          prompt: { type: 'string' }
        }
      },
      steps: [
        {
          id: 'chat',
          component: 'builtins://openai',
          input: {
            messages: [
              {
                role: 'user',
                content: { $from: { workflow: 'input' }, path: 'prompt' }
              }
            ]
          }
        }
      ],
      output: {
        response: { $from: { step: 'chat' }, path: 'choices[0].message.content' }
      }
    }
  }
]

export function FlowUploadDialog({ trigger, onSuccess }: FlowUploadDialogProps) {
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [description, setDescription] = useState('')
  const [flowContent, setFlowContent] = useState('{}')
  const [flowFormat, setFlowFormat] = useState<'json' | 'yaml'>('json')
  const [uploadMethod, setUploadMethod] = useState<'template' | 'file' | 'paste'>('template')
  const [selectedExample, setSelectedExample] = useState<string>('')

  const storeFlowMutation = useStoreFlow()

  // Load template data
  const loadTemplate = useCallback((templateName: string) => {
    const template = WORKFLOW_EXAMPLES.find(ex => ex.name === templateName)
    if (template) {
      if (template.format === 'yaml' || flowFormat === 'yaml') {
        // Convert to YAML format (simplified)
        const yamlContent = JSON.stringify(template.data, null, 2)
          .replace(/"/g, '')
          .replace(/,$/gm, '')
          .replace(/[\{\}]/g, '')
          .trim()
        setFlowContent(yamlContent)
        setFlowFormat('yaml')
      } else {
        // JSON format
        setFlowContent(JSON.stringify(template.data, null, 2))
        setFlowFormat('json')
      }
      setSelectedExample(templateName)
      
      // Auto-fill name from template if not set
      if (!name && template.data && typeof template.data === 'object' && 'name' in template.data) {
        setName(template.data.name as string)
      }
    }
  }, [flowFormat, name])

  // Handle format change
  useEffect(() => {
    if (selectedExample) {
      loadTemplate(selectedExample)
    }
  }, [flowFormat, selectedExample, loadTemplate])

  const handleFileUpload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0]
    if (file) {
      const reader = new FileReader()
      reader.onload = (e) => {
        const content = e.target?.result as string
        setFlowContent(content)
        
        // Detect format from file extension
        if (file.name.endsWith('.yaml') || file.name.endsWith('.yml')) {
          setFlowFormat('yaml')
        } else {
          setFlowFormat('json')
        }
        
        // Try to extract name from filename if not set
        if (!name) {
          const filename = file.name.replace(/\.(yaml|yml|json)$/, '')
          setName(filename)
        }
        
        setSelectedExample('') // Clear template selection
      }
      reader.readAsText(file)
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    
    if (!name.trim()) {
      toast.error('Flow name is required')
      return
    }
    
    if (!flowContent.trim()) {
      toast.error('Flow content is required')
      return
    }

    try {
      // Parse the flow content
      let flow: Record<string, unknown>
      try {
        if (flowFormat === 'json') {
          flow = JSON.parse(flowContent) as Record<string, unknown>
        } else {
          // Simple YAML parsing (similar to execution dialog)
          const yamlLines = flowContent.split('\n').filter(line => line.trim())
          flow = {}
          yamlLines.forEach(line => {
            const colonIndex = line.indexOf(':')
            if (colonIndex > 0) {
              const key = line.substring(0, colonIndex).trim()
              let value = line.substring(colonIndex + 1).trim()
              
              // Remove quotes if present
              if ((value.startsWith('"') && value.endsWith('"')) || 
                  (value.startsWith("'") && value.endsWith("'"))) {
                value = value.slice(1, -1)
              }
              
              // Try to parse as number or boolean
              if (value === 'true') {
                flow[key] = true
              } else if (value === 'false') {
                flow[key] = false
              } else if (!isNaN(Number(value)) && value !== '') {
                flow[key] = Number(value)
              } else {
                flow[key] = value
              }
            }
          })
        }
      } catch (error) {
        throw new Error(`Invalid ${flowFormat.toUpperCase()} format: ${error}`)
      }

      await storeFlowMutation.mutateAsync({
        name: name.trim(),
        description: description.trim() || undefined,
        flow,
      })

      toast.success(`Flow "${name}" created successfully!`)
      
      // Reset form
      resetForm()
      setOpen(false)
      
      onSuccess?.()
    } catch (error) {
      console.error('Failed to create flow:', error)
      toast.error(error instanceof Error ? error.message : 'Failed to create flow')
    }
  }

  const resetForm = () => {
    setName('')
    setDescription('')
    setFlowContent('{}')
    setFlowFormat('json')
    setUploadMethod('template')
    setSelectedExample('')
  }

  // Reset form when dialog closes
  useEffect(() => {
    if (!open) {
      resetForm()
    }
  }, [open])

  const isCreating = storeFlowMutation.isPending

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>
        {trigger}
      </DialogTrigger>
      
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center space-x-2">
            <Plus className="h-5 w-5" />
            <span>Create New Flow</span>
          </DialogTitle>
          <DialogDescription>
            Create a new workflow definition using templates, file upload, or direct editing
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-6 py-4">
          {/* Flow Metadata */}
          <Card>
            <CardHeader>
              <CardTitle className="text-base">Flow Information</CardTitle>
              <CardDescription>
                Basic metadata for your workflow
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                <div className="space-y-2">
                  <Label htmlFor="name">Flow Name *</Label>
                  <Input
                    id="name"
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="my-awesome-flow"
                    required
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="description">Description</Label>
                  <Input
                    id="description"
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    placeholder="Brief description..."
                  />
                </div>
              </div>
            </CardContent>
          </Card>

          {/* Templates Section */}
          {uploadMethod === 'template' && (
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center space-x-2 text-base">
                  <Lightbulb className="h-4 w-4" />
                  <span>Workflow Templates</span>
                </CardTitle>
                <CardDescription>
                  Start with a pre-built workflow template or create from scratch
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
                  {WORKFLOW_EXAMPLES.map((template) => (
                    <Button
                      key={template.name}
                      type="button"
                      variant={selectedExample === template.name ? "default" : "outline"}
                      size="sm"
                      onClick={() => loadTemplate(template.name)}
                      className="text-left justify-start h-auto p-3 min-h-[3rem] max-h-[5rem] overflow-hidden"
                    >
                      <div className="w-full overflow-hidden">
                        <div className="font-medium text-sm truncate">{template.name}</div>
                        {template.description && (
                          <div className="text-xs text-muted-foreground mt-1 line-clamp-2">
                            {template.description}
                          </div>
                        )}
                      </div>
                    </Button>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}

          {/* Creation Method Selection */}
          <div className="space-y-4">
            <Label>Creation Method</Label>
            <div className="flex gap-2 flex-wrap">
              <Button
                type="button"
                variant={uploadMethod === 'template' ? 'default' : 'outline'}
                size="sm"
                onClick={() => setUploadMethod('template')}
              >
                <Lightbulb className="mr-2 h-4 w-4" />
                Use Template
              </Button>
              <Button
                type="button"
                variant={uploadMethod === 'file' ? 'default' : 'outline'}
                size="sm"
                onClick={() => setUploadMethod('file')}
              >
                <FileText className="mr-2 h-4 w-4" />
                Upload File
              </Button>
              <Button
                type="button"
                variant={uploadMethod === 'paste' ? 'default' : 'outline'}
                size="sm"
                onClick={() => setUploadMethod('paste')}
              >
                <Code className="mr-2 h-4 w-4" />
                Write Code
              </Button>
            </div>
          </div>

          {/* File Upload */}
          {uploadMethod === 'file' && (
            <div className="space-y-4">
              <div className="space-y-2">
                <Label htmlFor="file">Flow File (.json, .yaml, .yml)</Label>
                <Input
                  id="file"
                  type="file"
                  accept=".json,.yaml,.yml"
                  onChange={handleFileUpload}
                />
                {flowContent && flowContent !== '{}' && (
                  <div className="mt-2 p-3 bg-muted rounded text-sm">
                    <strong>File loaded:</strong> {flowContent.length} characters
                  </div>
                )}
              </div>
            </div>
          )}

          {/* Flow Definition Editor */}
          {(uploadMethod === 'paste' || uploadMethod === 'template') && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <Label>Flow Definition</Label>
                <div className="flex items-center space-x-2">
                  <Label htmlFor="flow-format" className="text-sm">Format:</Label>
                  <Select 
                    value={flowFormat} 
                    onValueChange={(value) => setFlowFormat(value as 'json' | 'yaml')}
                  >
                    <SelectTrigger className="w-24">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="json">JSON</SelectItem>
                      <SelectItem value="yaml">YAML</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
              </div>
              
              <Textarea
                placeholder={`Enter workflow definition in ${flowFormat.toUpperCase()} format...`}
                className="min-h-[300px] font-mono text-sm"
                value={flowContent}
                onChange={(e) => {
                  setFlowContent(e.target.value)
                  setSelectedExample('') // Clear template selection when manually editing
                }}
              />
              
              <div className="flex items-center space-x-2">
                <Button type="button" variant="outline" size="sm">
                  <Code className="mr-2 h-4 w-4" />
                  Validate
                </Button>
                <Button type="button" variant="outline" size="sm">
                  <Upload className="mr-2 h-4 w-4" />
                  Load from File
                </Button>
              </div>
            </div>
          )}

          {/* Creation Summary */}
          <Card>
            <CardContent className="pt-6">
              <div className="flex items-center justify-between">
                <div className="space-y-1">
                  <div className="font-medium">Ready to Create</div>
                  <div className="text-sm text-muted-foreground">
                    Method: {uploadMethod === 'template' ? 'Template' : uploadMethod === 'file' ? 'File Upload' : 'Manual'}
                    {selectedExample && ` • Using ${selectedExample} template`}
                    {flowFormat && ` • ${flowFormat.toUpperCase()} format`}
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)} disabled={isCreating}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={isCreating || !name.trim() || !flowContent.trim() || flowContent === '{}'}>
            {isCreating ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Creating...
              </>
            ) : (
              <>
                <Plus className="mr-2 h-4 w-4" />
                Create Flow
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}