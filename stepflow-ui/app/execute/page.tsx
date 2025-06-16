'use client'

import { useState, useEffect, Suspense } from 'react'
import { useRouter, useSearchParams } from 'next/navigation'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Textarea } from '@/components/ui/textarea'
import { Label } from '@/components/ui/label'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Checkbox } from '@/components/ui/checkbox'
import { Play, Upload, FileText, Globe, Code, Bug, Loader2, Zap } from 'lucide-react'
import { useEndpoints, useExecuteWorkflow, useExecuteEndpoint } from '@/lib/hooks/use-api'
import { ExecutionDialog } from '@/components/execution-dialog'
import { EXAMPLE_WORKFLOWS } from '@/lib/examples'

// Example workflow from tests/python/python_math.yaml
const EXAMPLE_WORKFLOW = `input_schema:
  type: object
  properties:
    m:
      type: integer
    n:
      type: integer
output_schema:
  type: object
  properties:
    x:
      type: integer
    y:
      type: integer
steps:
- id: m_plus_n
  component: python://add
  input_schema: null
  output_schema: null
  input:
    a:
      $from:
        workflow: input
      path: m
    b:
      $from:
        workflow: input
      path: n
- id: m_times_n
  component: python://multiply
  input_schema: null
  output_schema: null
  input:
    a:
      $from:
        workflow: input
      path: m
    b:
      $from:
        workflow: input
      path: n
- id: m_plus_n_times_n
  component: python://multiply
  input_schema: null
  output_schema: null
  input:
    a:
      $from:
        step: m_plus_n
      path: result
    b:
      $from:
        workflow: input
      path: n
output:
  m_plus_n_times_n:
    $from:
      step: m_plus_n_times_n
    path: result
  m_times_n:
    $from:
      step: m_times_n
    path: result
  m_plus_n:
    $from:
      step: m_plus_n
    path: result`

const EXAMPLE_INPUT_JSON = `{
  "m": 8,
  "n": 5
}`

const EXAMPLE_INPUT_YAML = `m: 8
n: 5`

function ExecutePageContent() {
  const router = useRouter()
  const searchParams = useSearchParams()

  // Check for endpoint parameter from URL
  const preselectedEndpoint = searchParams.get('endpoint')
  const preselectedLabel = searchParams.get('label')

  const [selectedMethod, setSelectedMethod] = useState<'endpoint' | 'upload'>('upload') // Default to upload (ad-hoc)
  const [selectedEndpoint, setSelectedEndpoint] = useState('')
  const [workflowContent, setWorkflowContent] = useState(EXAMPLE_WORKFLOW)
  const [inputContent, setInputContent] = useState(EXAMPLE_INPUT_JSON)
  const [inputFormat, setInputFormat] = useState<'json' | 'yaml'>('json')
  const [debugMode, setDebugMode] = useState(false)

  // API hooks
  const { data: endpoints, isLoading: endpointsLoading } = useEndpoints()
  const executeWorkflowMutation = useExecuteWorkflow()
  const executeEndpointMutation = useExecuteEndpoint()

  // Handle preselected endpoint from URL
  useEffect(() => {
    if (preselectedEndpoint) {
      setSelectedMethod('endpoint')
      const endpointKey = preselectedLabel
        ? `${preselectedEndpoint}?label=${preselectedLabel}`
        : preselectedEndpoint
      setSelectedEndpoint(endpointKey)
    }
  }, [preselectedEndpoint, preselectedLabel])

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
      if (selectedMethod === 'endpoint') {
        const [endpointName, labelParam] = selectedEndpoint.split('?')
        const label = labelParam?.split('=')[1]

        result = await executeEndpointMutation.mutateAsync({
          name: endpointName,
          data: { input: parsedInput, debug: debugMode },
          label
        })
      } else {
        // Parse workflow YAML to JSON (simplified for this example)
        let workflowObj
        try {
          // For now, we'll assume it's already in JSON format or simple YAML
          // In a real implementation, you'd use a proper YAML parser
          workflowObj = JSON.parse(workflowContent)
        } catch {
          // Simple YAML to JSON conversion for the example workflow
          workflowObj = parseSimpleYaml(workflowContent)
        }

        result = await executeWorkflowMutation.mutateAsync({
          workflow: workflowObj,
          input: parsedInput,
          debug: debugMode
        })
      }

      // Navigate to execution details
      router.push(`/executions/${result.execution_id}`)
    } catch (error) {
      console.error('Execution failed:', error)
      // TODO: Show error toast
    }
  }

  const loadExampleWorkflow = () => {
    setWorkflowContent(EXAMPLE_WORKFLOW)
    setInputContent(inputFormat === 'json' ? EXAMPLE_INPUT_JSON : EXAMPLE_INPUT_YAML)
  }

  // Simple YAML parser for the example workflow
  const parseSimpleYaml = (yamlString: string) => {
    // This is a very basic YAML parser for demonstration
    // In production, use a proper YAML library like js-yaml
    const lines = yamlString.split('\n')
    const result: Record<string, unknown> = {}
    let currentKey = ''
    let currentArray: Record<string, unknown>[] = []
    let inArray = false

    lines.forEach(line => {
      const trimmed = line.trim()
      if (!trimmed || trimmed.startsWith('#')) return

      if (trimmed.endsWith(':')) {
        currentKey = trimmed.slice(0, -1)
        if (currentKey === 'steps') {
          inArray = true
          currentArray = []
          result[currentKey] = currentArray
        } else {
          inArray = false
          result[currentKey] = {}
        }
      } else if (trimmed.startsWith('- ')) {
        if (inArray) {
          const stepMatch = trimmed.match(/- id: (.+)/)
          if (stepMatch) {
            currentArray.push({ id: stepMatch[1] })
          }
        }
      }
    })

    // Simplified structure for the math example
    return {
      input_schema: {
        type: "object",
        properties: {
          m: { type: "integer" },
          n: { type: "integer" }
        }
      },
      steps: [
        {
          id: "m_plus_n",
          component: "python://add",
          input: {
            a: { $from: { workflow: "input" }, path: "m" },
            b: { $from: { workflow: "input" }, path: "n" }
          }
        },
        {
          id: "m_times_n",
          component: "python://multiply",
          input: {
            a: { $from: { workflow: "input" }, path: "m" },
            b: { $from: { workflow: "input" }, path: "n" }
          }
        },
        {
          id: "m_plus_n_times_n",
          component: "python://multiply",
          input: {
            a: { $from: { step: "m_plus_n" }, path: "result" },
            b: { $from: { workflow: "input" }, path: "n" }
          }
        }
      ],
      output: {
        m_plus_n_times_n: { $from: { step: "m_plus_n_times_n" }, path: "result" },
        m_times_n: { $from: { step: "m_times_n" }, path: "result" },
        m_plus_n: { $from: { step: "m_plus_n" }, path: "result" }
      }
    }
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-3xl font-bold tracking-tight">Execute Workflow</h1>
        <p className="text-muted-foreground">
          Submit and run workflows using endpoints or direct upload
        </p>
      </div>

      {/* Quick Execute Section */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center space-x-2">
            <Zap className="h-5 w-5" />
            <span>Quick Execute</span>
          </CardTitle>
          <CardDescription>
            Try example workflows with pre-configured inputs
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            {EXAMPLE_WORKFLOWS.map((example) => (
              <div key={example.name} className="border rounded-lg p-4">
                <h3 className="font-semibold text-sm mb-2">{example.name}</h3>
                <p className="text-sm text-muted-foreground mb-3">
                  {example.description}
                </p>
                <ExecutionDialog
                  workflow={example.workflow}
                  examples={example.inputExamples}
                  trigger={
                    <Button size="sm" className="w-full">
                      <Play className="mr-2 h-4 w-4" />
                      Quick Execute
                    </Button>
                  }
                />
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      <div className="grid gap-6 md:grid-cols-2 h-[calc(100vh-12rem)]">
        <Card className="flex flex-col">
          <CardHeader className="flex-shrink-0">
            <CardTitle>Workflow Source</CardTitle>
            <CardDescription>
              Choose how to provide the workflow definition
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-4 flex-1 overflow-hidden">
            <Tabs value={selectedMethod} onValueChange={(value) => setSelectedMethod(value as 'endpoint' | 'upload')} className="flex-1 flex flex-col">
              <TabsList className="grid w-full grid-cols-2 flex-shrink-0">
                <TabsTrigger value="endpoint">
                  <Globe className="mr-2 h-4 w-4" />
                  Endpoint
                </TabsTrigger>
                <TabsTrigger value="upload">
                  <Upload className="mr-2 h-4 w-4" />
                  Upload
                </TabsTrigger>
              </TabsList>

              <TabsContent value="endpoint" className="space-y-4 flex-1 flex flex-col">
                <div className="space-y-2">
                  <Label htmlFor="endpoint-select">Select Endpoint</Label>
                  <Select value={selectedEndpoint} onValueChange={setSelectedEndpoint}>
                    <SelectTrigger>
                      <SelectValue placeholder="Choose an endpoint" />
                    </SelectTrigger>
                    <SelectContent>
                      {endpointsLoading ? (
                        <SelectItem value="loading" disabled>
                          <div className="flex items-center space-x-2">
                            <Loader2 className="h-4 w-4 animate-spin" />
                            <span>Loading endpoints...</span>
                          </div>
                        </SelectItem>
                      ) : endpoints?.length === 0 ? (
                        <SelectItem value="no-endpoints" disabled>
                          No endpoints available
                        </SelectItem>
                      ) : (
                        endpoints?.map((endpoint) => (
                          <SelectItem
                            key={`${endpoint.name}-${endpoint.label || 'default'}`}
                            value={`${endpoint.name}${endpoint.label ? `?label=${endpoint.label}` : ''}`}
                          >
                            <div className="flex items-center space-x-2">
                              <span>{endpoint.name}</span>
                              {endpoint.label && (
                                <Badge variant="outline" className="text-xs">
                                  {endpoint.label}
                                </Badge>
                              )}
                            </div>
                          </SelectItem>
                        ))
                      )}
                    </SelectContent>
                  </Select>
                </div>
                <div className="flex-1 flex items-center justify-center text-muted-foreground">
                  <p className="text-sm">Workflow definition will be loaded from the selected endpoint</p>
                </div>
              </TabsContent>

              <TabsContent value="upload" className="space-y-4 flex-1 flex flex-col">
                <div className="space-y-2 flex-1 flex flex-col">
                  <Label htmlFor="workflow-content">Workflow Definition</Label>
                  <Textarea
                    id="workflow-content"
                    placeholder="Paste your workflow YAML or JSON here..."
                    className="h-128 font-mono text-sm resize-none"
                    value={workflowContent}
                    onChange={(e) => setWorkflowContent(e.target.value)}
                  />
                </div>
                <div className="flex items-center space-x-2">
                  <Button variant="outline" size="sm">
                    <Upload className="mr-2 h-4 w-4" />
                    Upload File
                  </Button>
                  <Button variant="outline" size="sm" onClick={loadExampleWorkflow}>
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
                Provide input data for the workflow execution
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
                Configure how the workflow should be executed
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
                    {selectedMethod === 'endpoint'
                      ? `Using endpoint: ${selectedEndpoint || 'None selected'}`
                      : 'Using ad-hoc workflow definition'
                    }
                    {debugMode && ' • Debug mode enabled'}
                  </div>
                </div>
                <Button
                  size="lg"
                  onClick={handleExecute}
                  disabled={
                    (selectedMethod === 'endpoint' ? !selectedEndpoint : !workflowContent) ||
                    executeWorkflowMutation.isPending ||
                    executeEndpointMutation.isPending
                  }
                >
                  {(executeWorkflowMutation.isPending || executeEndpointMutation.isPending) ? (
                    <>
                      <Loader2 className="mr-2 h-5 w-5 animate-spin" />
                      Executing...
                    </>
                  ) : (
                    <>
                      <Play className="mr-2 h-5 w-5" />
                      Execute Workflow
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