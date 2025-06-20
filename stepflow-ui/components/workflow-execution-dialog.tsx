'use client'

import { useState, useEffect, useCallback } from 'react'
import { useRouter } from 'next/navigation'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle, DialogTrigger } from '@/components/ui/dialog'
import { Textarea } from '@/components/ui/textarea'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Checkbox } from '@/components/ui/checkbox'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Play, Upload, Bug, Loader2, Code, Lightbulb } from 'lucide-react'
import { useExecuteFlow, useFlow } from '@/lib/hooks/use-flow-api'
import { COMMON_INPUT_EXAMPLES, getInputExamplesForComponents, type InputExample } from '@/lib/examples'

interface WorkflowExecutionDialogProps {
  // Named workflow execution
  workflowName: string
  // Dialog trigger element
  trigger?: React.ReactNode
  // Dialog state control
  open?: boolean
  onOpenChange?: (open: boolean) => void
}

// Helper function to get relevant examples based on workflow
function getRelevantExamples(workflow?: { flow?: { steps?: Array<{ component: string }> } }): InputExample[] {
  // If the workflow has steps, try to get component-based examples
  if (workflow?.flow?.steps) {
    const components = workflow.flow.steps.map((step) => step.component)
    const relevantExamples = getInputExamplesForComponents(components)
    return relevantExamples.length > 0 ? relevantExamples : COMMON_INPUT_EXAMPLES
  }
  
  return COMMON_INPUT_EXAMPLES
}

export function WorkflowExecutionDialog({ 
  workflowName,
  trigger, 
  open: controlledOpen, 
  onOpenChange: controlledOnOpenChange
}: WorkflowExecutionDialogProps) {
  const router = useRouter()
  
  // Dialog state management
  const [internalOpen, setInternalOpen] = useState(false)
  const isControlled = controlledOpen !== undefined
  const open = isControlled ? controlledOpen : internalOpen
  const setOpen = isControlled ? (controlledOnOpenChange || (() => {})) : setInternalOpen

  // Form state
  const [inputContent, setInputContent] = useState('{}')
  const [inputFormat, setInputFormat] = useState<'json' | 'yaml'>('json')
  const [debugMode, setDebugMode] = useState(false)
  const [selectedExample, setSelectedExample] = useState<string>('')

  // API hooks
  const executeFlowMutation = useExecuteFlow()
  const flowQuery = useFlow(workflowName)

  // Flow data
  const flowData = flowQuery.data
  const isLoadingFlow = flowQuery.isLoading
  
  const executionTitle = `Execute ${workflowName}`
  const executionDescription = flowData?.flow?.description || 'Execute flow with input data'
  
  // Get examples from flow
  const relevantExamples = getRelevantExamples(flowData)

  // Load example data
  const loadExample = useCallback((exampleName: string) => {
    const example = relevantExamples.find(ex => ex.name === exampleName)
    if (example) {
      if (example.format === 'yaml' || inputFormat === 'yaml') {
        // Convert to YAML format
        const yamlContent = Object.entries(example.data)
          .map(([key, value]) => `${key}: ${typeof value === 'string' ? `"${value}"` : value}`)
          .join('\n')
        setInputContent(yamlContent)
        setInputFormat('yaml')
      } else {
        // JSON format
        setInputContent(JSON.stringify(example.data, null, 2))
        setInputFormat('json')
      }
      setSelectedExample(exampleName)
    }
  }, [relevantExamples, inputFormat])

  // Handle format change
  useEffect(() => {
    if (selectedExample) {
      loadExample(selectedExample)
    }
  }, [inputFormat, selectedExample, loadExample])

  // Handle execution
  const handleExecute = async () => {
    try {
      let parsedInput: Record<string, unknown>
      
      try {
        if (inputFormat === 'json') {
          parsedInput = JSON.parse(inputContent)
        } else {
          // Simple YAML parsing
          const yamlLines = inputContent.split('\n').filter(line => line.trim())
          parsedInput = {}
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
                parsedInput[key] = true
              } else if (value === 'false') {
                parsedInput[key] = false
              } else if (!isNaN(Number(value)) && value !== '') {
                parsedInput[key] = Number(value)
              } else {
                parsedInput[key] = value
              }
            }
          })
        }
      } catch (error) {
        throw new Error(`Invalid ${inputFormat.toUpperCase()} format in input: ${error}`)
      }

      const result = await executeFlowMutation.mutateAsync({
        name: workflowName,
        input: parsedInput,
        debug: debugMode
      })

      // Close dialog and navigate to results
      setOpen(false)
      router.push(`/runs/${result.runId}`)
    } catch (error) {
      console.error('Execution failed:', error)
      // TODO: Show error toast notification
    }
  }

  // Reset form when dialog closes
  useEffect(() => {
    if (!open) {
      setInputContent('{}')
      setInputFormat('json')
      setDebugMode(false)
      setSelectedExample('')
    }
  }, [open])

  const defaultTrigger = (
    <Button>
      <Play className="mr-2 h-4 w-4" />
      Execute
    </Button>
  )

  const isExecuting = executeFlowMutation.isPending

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      {trigger && (
        <DialogTrigger asChild>
          {trigger}
        </DialogTrigger>
      )}
      {!trigger && !isControlled && (
        <DialogTrigger asChild>
          {defaultTrigger}
        </DialogTrigger>
      )}
      
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle className="flex items-center space-x-2">
            <Play className="h-5 w-5" />
            <span>{executionTitle}</span>
          </DialogTitle>
          <DialogDescription>
            {executionDescription}
          </DialogDescription>
        </DialogHeader>

        <div className="grid gap-6 py-4">
          {/* Loading state for flow data */}
          {isLoadingFlow && (
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-center space-x-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  <span>Loading flow information...</span>
                </div>
              </CardContent>
            </Card>
          )}

          {/* Examples Section */}
          {!isLoadingFlow && relevantExamples.length > 0 && (
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center space-x-2 text-base">
                  <Lightbulb className="h-4 w-4" />
                  <span>Input Examples</span>
                </CardTitle>
                <CardDescription>
                  Choose from common input patterns or create your own
                </CardDescription>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4 gap-2">
                  {relevantExamples.map((example) => (
                    <Button
                      key={example.name}
                      variant={selectedExample === example.name ? "default" : "outline"}
                      size="sm"
                      onClick={() => loadExample(example.name)}
                      className="text-left justify-start h-auto p-3 min-h-[3rem] max-h-[5rem] overflow-hidden"
                    >
                      <div className="w-full overflow-hidden">
                        <div className="font-medium text-sm truncate">{example.name}</div>
                        {example.description && (
                          <div className="text-xs text-muted-foreground mt-1 line-clamp-2">
                            {example.description}
                          </div>
                        )}
                      </div>
                    </Button>
                  ))}
                </div>
              </CardContent>
            </Card>
          )}

          {/* Input Data Section */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <Label>Input Data</Label>
              <div className="flex items-center space-x-2">
                <Label htmlFor="input-format" className="text-sm">Format:</Label>
                <Select 
                  value={inputFormat} 
                  onValueChange={(value) => setInputFormat(value as 'json' | 'yaml')}
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
              placeholder={`Enter input data in ${inputFormat.toUpperCase()} format...`}
              className="min-h-[200px] font-mono text-sm"
              value={inputContent}
              onChange={(e) => {
                setInputContent(e.target.value)
                setSelectedExample('') // Clear selection when manually editing
              }}
            />
            
            <div className="flex items-center space-x-2">
              <Button variant="outline" size="sm">
                <Upload className="mr-2 h-4 w-4" />
                Upload File
              </Button>
              <Button variant="outline" size="sm">
                <Code className="mr-2 h-4 w-4" />
                Validate
              </Button>
            </div>
          </div>

          {/* Execution Options */}
          <div className="space-y-4">
            <Label>Execution Options</Label>
            <div className="flex items-center space-x-2">
              <Checkbox
                id="debug-mode"
                checked={debugMode}
                onCheckedChange={(checked) => setDebugMode(checked as boolean)}
              />
              <Label htmlFor="debug-mode" className="flex items-center space-x-2 cursor-pointer">
                <Bug className="h-4 w-4 text-orange-500" />
                <span>Enable Debug Mode</span>
              </Label>
            </div>
            {debugMode && (
              <div className="text-sm text-muted-foreground bg-orange-50 border border-orange-200 rounded-lg p-3">
                <p className="flex items-center space-x-2">
                  <Bug className="h-4 w-4 text-orange-500" />
                  <span>Debug mode allows step-by-step execution and inspection of intermediate results.</span>
                </p>
              </div>
            )}
          </div>

          {/* Execution Summary */}
          <Card>
            <CardContent className="pt-6">
              <div className="flex items-center justify-between">
                <div className="space-y-1">
                  <div className="font-medium">Ready to Execute</div>
                  <div className="text-sm text-muted-foreground">
                    Flow: {workflowName}
                    {debugMode && ' • Debug mode enabled'}
                    {selectedExample && ` • Using ${selectedExample} example`}
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)} disabled={isExecuting || isLoadingFlow}>
            Cancel
          </Button>
          <Button onClick={handleExecute} disabled={isExecuting || isLoadingFlow}>
            {isExecuting ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Executing...
              </>
            ) : isLoadingFlow ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Loading...
              </>
            ) : (
              <>
                <Play className="mr-2 h-4 w-4" />
                Execute
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}