'use client'

import { useState, useEffect, Suspense } from 'react'
import { useRouter, useSearchParams } from 'next/navigation'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Checkbox } from '@/components/ui/checkbox'
import { Play, Upload, FileText, Workflow, Code, Bug, Loader2 } from 'lucide-react'
import { useFlows, useExecuteFlow, useExecuteAdHocWorkflow } from '@/lib/hooks/use-flow-api'

// Example workflow from tests/python/python_math.yaml
const EXAMPLE_WORKFLOW = `{
  "name": "Math Operations",
  "description": "Demonstrates basic math operations",
  "input_schema": {
    "type": "object",
    "properties": {
      "m": { "type": "integer" },
      "n": { "type": "integer" }
    }
  },
  "steps": [
    {
      "id": "m_plus_n",
      "component": "python://add",
      "input": {
        "a": { "$from": { "workflow": "input" }, "path": "m" },
        "b": { "$from": { "workflow": "input" }, "path": "n" }
      }
    },
    {
      "id": "m_times_n",
      "component": "python://multiply",
      "input": {
        "a": { "$from": { "workflow": "input" }, "path": "m" },
        "b": { "$from": { "workflow": "input" }, "path": "n" }
      }
    }
  ],
  "output": {
    "sum": { "$from": { "step": "m_plus_n" }, "path": "result" },
    "product": { "$from": { "step": "m_times_n" }, "path": "result" }
  }
}`

const EXAMPLE_INPUT_JSON = `{
  "m": 8,
  "n": 5
}`

const EXAMPLE_INPUT_YAML = `m: 8
n: 5`

function ExecutePageContent() {
  const router = useRouter()
  const searchParams = useSearchParams()

  // Check for flow parameter from URL
  const preselectedFlow = searchParams.get('flow') ? decodeURIComponent(searchParams.get('flow')!) : null
  const preselectedLabel = searchParams.get('label') ? decodeURIComponent(searchParams.get('label')!) : null

  const [selectedMethod, setSelectedMethod] = useState<'flow' | 'upload'>('upload') // Default to upload (ad-hoc)
  const [selectedFlow, setSelectedFlow] = useState('')
  const [selectedLabel, setSelectedLabel] = useState('')
  const [flowContent, setFlowContent] = useState(EXAMPLE_WORKFLOW)
  const [inputContent, setInputContent] = useState(EXAMPLE_INPUT_JSON)
  const [inputFormat, setInputFormat] = useState<'json' | 'yaml'>('json')
  const [debugMode, setDebugMode] = useState(false)

  // API hooks
  const { data: flows, isLoading: flowsLoading } = useFlows()
  const executeFlowMutation = useExecuteFlow()
  const executeAdHocMutation = useExecuteAdHocWorkflow()
  
  // Extract flow names from the flows list
  const flowNames = flows?.map(w => w.name) || []

  // Handle preselected flow from URL
  useEffect(() => {
    if (preselectedFlow) {
      setSelectedMethod('flow')
      setSelectedFlow(preselectedFlow)
      if (preselectedLabel) {
        setSelectedLabel(preselectedLabel)
      }
    }
  }, [preselectedFlow, preselectedLabel])

  // Update input content when format changes
  useEffect(() => {
    if (inputFormat === 'json' && inputContent === EXAMPLE_INPUT_YAML) {
      setInputContent(EXAMPLE_INPUT_JSON)
    } else if (inputFormat === 'yaml' && inputContent === EXAMPLE_INPUT_JSON) {
      setInputContent(EXAMPLE_INPUT_YAML)
    }
  }, [inputFormat, inputContent])

  const handleExecute = async () => {
    try {
      let parsedInput: Record<string, unknown>
      try {
        if (inputFormat === 'json') {
          parsedInput = JSON.parse(inputContent)
        } else {
          // For YAML, we'll need to parse it - for now, convert simple YAML to JSON
          const yamlLines = inputContent.split('\n').filter(line => line.trim())
          parsedInput = {}
          yamlLines.forEach(line => {
            const [key, value] = line.split(':').map(s => s.trim())
            if (key && value) {
              // Try to parse as number, otherwise keep as string
              const numValue = Number(value)
              parsedInput[key] = isNaN(numValue) ? value : numValue
            }
          })
        }
      } catch {
        throw new Error(`Invalid ${inputFormat.toUpperCase()} format in input`)
      }

      let result
      if (selectedMethod === 'flow') {
        // Execute by flow name (labels not yet supported in new API)
        result = await executeFlowMutation.mutateAsync({
          name: selectedFlow,
          input: parsedInput,
          debug: debugMode
        })
      } else {
        // Execute ad-hoc workflow
        let workflow: Record<string, unknown>
        try {
          workflow = JSON.parse(flowContent)
        } catch {
          throw new Error('Invalid JSON format in workflow definition')
        }

        result = await executeAdHocMutation.mutateAsync({
          workflow,
          input: parsedInput,
          debug: debugMode
        })
      }

      // Navigate to run details using the new runId field
      router.push(`/runs/${result.runId}`)
    } catch (error) {
      console.error('Execution failed:', error)
      // TODO: Show error toast
    }
  }

  const loadExampleFlow = () => {
    setFlowContent(EXAMPLE_WORKFLOW)
    setInputContent(inputFormat === 'json' ? EXAMPLE_INPUT_JSON : EXAMPLE_INPUT_YAML)
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">Execute Flow</h1>
        <p className="text-muted-foreground">
          Submit and run flows using named flows or direct upload
        </p>
      </div>

      {/* Quick Execute Section - TODO: Implement ad-hoc execution for examples */}
      {/* 
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center space-x-2">
            <Zap className="h-5 w-5" />
            <span>Quick Execute</span>
          </CardTitle>
          <CardDescription>
            Try example flows with pre-configured inputs
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="text-center text-muted-foreground p-8">
            <p>Ad-hoc execution examples coming soon</p>
          </div>
        </CardContent>
      </Card>
      */}

      <div className="grid gap-6 md:grid-cols-2 h-[calc(100vh-12rem)]">
        <Card className="flex flex-col">
          <CardHeader className="flex-shrink-0">
            <CardTitle>Flow Source</CardTitle>
            <CardDescription>
              Choose how to provide the flow definition
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4 flex-1 overflow-hidden">
            <Tabs value={selectedMethod} onValueChange={(value) => setSelectedMethod(value as 'flow' | 'upload')} className="flex-1 flex flex-col">
              <TabsList className="grid w-full grid-cols-2 flex-shrink-0">
                <TabsTrigger value="flow">
                  <Workflow className="mr-2 h-4 w-4" />
                  Named Flow
                </TabsTrigger>
                <TabsTrigger value="upload">
                  <Upload className="mr-2 h-4 w-4" />
                  Upload
                </TabsTrigger>
              </TabsList>

              <TabsContent value="flow" className="space-y-4 flex-1 flex flex-col">
                <div className="space-y-2">
                  <Label htmlFor="flow-select">Select Flow</Label>
                  <Select value={selectedFlow} onValueChange={setSelectedFlow}>
                    <SelectTrigger>
                      <SelectValue placeholder="Choose a flow" />
                    </SelectTrigger>
                    <SelectContent>
                      {flowsLoading ? (
                        <SelectItem value="loading" disabled>
                          <div className="flex items-center space-x-2">
                            <Loader2 className="h-4 w-4 animate-spin" />
                            <span>Loading flows...</span>
                          </div>
                        </SelectItem>
                      ) : flowNames?.length === 0 ? (
                        <SelectItem value="no-flows" disabled>
                          No flows available
                        </SelectItem>
                      ) : (
                        flowNames?.map((name) => (
                          <SelectItem key={name} value={name}>
                            {name}
                          </SelectItem>
                        ))
                      )}
                    </SelectContent>
                  </Select>
                </div>

                {selectedFlow && (
                  <div className="space-y-2">
                    <Label htmlFor="label-input">Label (optional)</Label>
                    <input
                      id="label-input"
                      type="text"
                      placeholder="e.g., production, staging, latest"
                      className="w-full px-3 py-2 border border-input rounded-md"
                      value={selectedLabel}
                      onChange={(e) => setSelectedLabel(e.target.value)}
                    />
                    <p className="text-sm text-muted-foreground">
                      Leave empty to use the latest version
                    </p>
                  </div>
                )}

                <div className="flex-1 flex items-center justify-center text-muted-foreground">
                  <p className="text-sm">
                    {selectedFlow
                      ? `Will execute flow: ${selectedFlow}${selectedLabel ? ` (${selectedLabel})` : ' (latest)'}`
                      : 'Select a flow to execute'
                    }
                  </p>
                </div>
              </TabsContent>

              <TabsContent value="upload" className="space-y-4 flex-1 flex flex-col">
                <div className="space-y-2 flex-1 flex flex-col">
                  <Label htmlFor="flow-content">Flow Definition (JSON)</Label>
                  <Textarea
                    id="flow-content"
                    placeholder="Paste your flow JSON here..."
                    className="flex-1 font-mono text-sm resize-none"
                    value={flowContent}
                    onChange={(e) => setFlowContent(e.target.value)}
                  />
                </div>
                <div className="flex items-center space-x-2">
                  <Button variant="outline" size="sm">
                    <Upload className="mr-2 h-4 w-4" />
                    Upload File
                  </Button>
                  <Button variant="outline" size="sm" onClick={loadExampleFlow}>
                    <FileText className="mr-2 h-4 w-4" />
                    Load Example
                  </Button>
                </div>
              </TabsContent>
            </Tabs>
          </CardContent>
        </Card>

        <div className="space-y-6 flex flex-col">
          <Card className="flex-1 flex flex-col">
            <CardHeader className="flex-shrink-0">
              <CardTitle>Input Data</CardTitle>
              <CardDescription>
                Provide input data for the flow execution
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 flex-1 flex flex-col">
              <div className="space-y-2 flex-1 flex flex-col">
                <div className="flex items-center space-x-2">
                  <Label htmlFor="input-format">Format</Label>
                  <Select value={inputFormat} onValueChange={(value) => setInputFormat(value as 'json' | 'yaml')}>
                    <SelectTrigger className="w-32">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="json">JSON</SelectItem>
                      <SelectItem value="yaml">YAML</SelectItem>
                    </SelectContent>
                  </Select>
                </div>
                <Textarea
                  id="input-content"
                  placeholder={`Enter input data in ${inputFormat.toUpperCase()} format...`}
                  className="font-mono text-sm resize-none h-64"
                  value={inputContent}
                  onChange={(e) => setInputContent(e.target.value)}
                />
              </div>
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
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>Execution Options</CardTitle>
              <CardDescription>
                Configure how the flow should be executed
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
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

              <div className="flex items-center justify-between">
                <div className="space-y-1">
                  <div className="font-medium">Ready to Execute</div>
                  <div className="text-sm text-muted-foreground">
                    {selectedMethod === 'flow'
                      ? `Using flow: ${selectedFlow || 'None selected'}${selectedLabel ? ` (${selectedLabel})` : ''}`
                      : 'Using ad-hoc flow definition'
                    }
                    {debugMode && ' • Debug mode enabled'}
                  </div>
                </div>
                <Button
                  size="lg"
                  onClick={handleExecute}
                  disabled={
                    (selectedMethod === 'flow' ? !selectedFlow : !flowContent) ||
                    executeFlowMutation.isPending ||
                    executeAdHocMutation.isPending
                  }
                >
                  {(executeFlowMutation.isPending || executeAdHocMutation.isPending) ? (
                    <>
                      <Loader2 className="mr-2 h-5 w-5 animate-spin" />
                      Executing...
                    </>
                  ) : (
                    <>
                      <Play className="mr-2 h-5 w-5" />
                      Execute Flow
                    </>
                  )}
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  )
}

export default function ExecutePage() {
  return (
    <Suspense fallback={
      <div className="flex items-center justify-center h-64">
        <div className="flex items-center space-x-2">
          <Loader2 className="h-6 w-6 animate-spin" />
          <span>Loading...</span>
        </div>
      </div>
    }>
      <ExecutePageContent />
    </Suspense>
  )
}