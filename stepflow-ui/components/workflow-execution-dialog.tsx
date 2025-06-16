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
import { useExecuteWorkflow, useExecuteWorkflowByName, useExecuteWorkflowByLabel, useLatestWorkflowByName, useWorkflowByLabel } from '@/lib/hooks/use-api'
import { COMMON_INPUT_EXAMPLES, getInputExamplesForComponents, type InputExample } from '@/lib/examples'
import type { Workflow, ExampleInput } from '@/lib/api'

interface WorkflowExecutionDialogProps {
  // Named workflow execution (latest version)
  workflowName?: string
  // Labeled workflow execution
  workflowLabel?: string
  // Ad-hoc workflow execution
  workflow?: Workflow
  // Dialog trigger element
  trigger?: React.ReactNode
  // Dialog state control
  open?: boolean
  onOpenChange?: (open: boolean) => void
  // Examples
  examples?: InputExample[]
}

// Helper function to convert backend ExampleInput to UI InputExample format
function convertBackendExamples(backendExamples: ExampleInput[]): InputExample[] {
  return backendExamples.map(example => ({
    name: example.name,
    description: example.description,
    data: example.input,
    format: 'json' as const
  }))
}

// Helper function to get relevant examples based on workflow
function getRelevantExamples(workflow?: Workflow, backendExamples?: ExampleInput[]): InputExample[] {
  // If we have examples from the backend, use those first
  if (backendExamples && backendExamples.length > 0) {
    return convertBackendExamples(backendExamples)
  }

  // If the workflow has examples directly, use those
  if (workflow?.examples && workflow.examples.length > 0) {
    return convertBackendExamples(workflow.examples)
  }

  // Fallback to component-based examples for backward compatibility
  if (workflow?.steps) {
    const components = workflow.steps.map(step => step.component)
    const relevantExamples = getInputExamplesForComponents(components)
    return relevantExamples.length > 0 ? relevantExamples : COMMON_INPUT_EXAMPLES
  }
  
  return COMMON_INPUT_EXAMPLES
}

export function WorkflowExecutionDialog({ 
  workflowName,
  workflowLabel,
  workflow, 
  trigger, 
  open: controlledOpen, 
  onOpenChange: controlledOnOpenChange,
  examples
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
  const executeWorkflowMutation = useExecuteWorkflow()
  const executeWorkflowByNameMutation = useExecuteWorkflowByName()
  const executeWorkflowByLabelMutation = useExecuteWorkflowByLabel()

  // Fetch workflow data based on execution type
  const latestWorkflowQuery = useLatestWorkflowByName(workflowName || '')
  const labeledWorkflowQuery = useWorkflowByLabel(workflowName || '', workflowLabel || '')

  // Determine execution mode and workflow data
  const isNamedExecution = !!workflowName && !workflowLabel
  const isLabeledExecution = !!workflowName && !!workflowLabel

  let workflowForExecution: Workflow | undefined
  let workflowData: { workflow: Workflow; workflow_hash: string; all_examples: ExampleInput[] } | undefined
  let isLoadingWorkflow = false

  if (isLabeledExecution) {
    workflowData = labeledWorkflowQuery.data
    workflowForExecution = workflowData?.workflow
    isLoadingWorkflow = labeledWorkflowQuery.isLoading
  } else if (isNamedExecution) {
    workflowData = latestWorkflowQuery.data
    workflowForExecution = workflowData?.workflow
    isLoadingWorkflow = latestWorkflowQuery.isLoading
  } else {
    workflowForExecution = workflow
  }
  
  const executionTitle = isLabeledExecution
    ? `Execute ${workflowName} (${workflowLabel})`
    : isNamedExecution
    ? `Execute ${workflowName} (latest)`
    : workflowForExecution?.name || 'Execute Workflow'
  
  const executionDescription = workflowForExecution?.description || 'Execute workflow with input data'
  
  // Get examples from multiple sources with priority:
  // 1. Explicitly provided examples
  // 2. Examples from workflow API response
  // 3. Examples from direct workflow
  // 4. Fallback to component-based examples
  const backendExamples = workflowData?.all_examples
  const relevantExamples = examples || getRelevantExamples(workflowForExecution, backendExamples)

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

      let result
      if (isLabeledExecution) {
        result = await executeWorkflowByLabelMutation.mutateAsync({
          name: workflowName!,
          label: workflowLabel!,
          data: { input: parsedInput, debug: debugMode }
        })
      } else if (isNamedExecution) {
        result = await executeWorkflowByNameMutation.mutateAsync({
          name: workflowName!,
          data: { input: parsedInput, debug: debugMode }
        })
      } else if (workflowForExecution) {
        result = await executeWorkflowMutation.mutateAsync({
          workflow: workflowForExecution,
          input: parsedInput,
          debug: debugMode
        })
      } else {
        throw new Error('No workflow specified for execution')
      }

      // Close dialog and navigate to results
      setOpen(false)
      router.push(`/executions/${result.execution_id}`)
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

  const isExecuting = executeWorkflowMutation.isPending || 
                     executeWorkflowByNameMutation.isPending || 
                     executeWorkflowByLabelMutation.isPending

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
          {/* Loading state for workflow data */}
          {isLoadingWorkflow && (
            <Card>
              <CardContent className="pt-6">
                <div className="flex items-center space-x-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  <span>Loading workflow information...</span>
                </div>
              </CardContent>
            </Card>
          )}

          {/* Examples Section */}
          {!isLoadingWorkflow && relevantExamples.length > 0 && (
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
                    {isLabeledExecution
                      ? `Workflow: ${workflowName} (${workflowLabel})`
                      : isNamedExecution
                      ? `Workflow: ${workflowName} (latest)`
                      : 'Ad-hoc workflow execution'
                    }
                    {debugMode && ' • Debug mode enabled'}
                    {selectedExample && ` • Using ${selectedExample} example`}
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => setOpen(false)} disabled={isExecuting || isLoadingWorkflow}>
            Cancel
          </Button>
          <Button onClick={handleExecute} disabled={isExecuting || isLoadingWorkflow}>
            {isExecuting ? (
              <>
                <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                Executing...
              </>
            ) : isLoadingWorkflow ? (
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