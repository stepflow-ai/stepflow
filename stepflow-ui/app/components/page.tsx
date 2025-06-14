import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Zap, Package, Settings, Info, ExternalLink } from 'lucide-react'

const mockComponents = [
  {
    name: 'openai',
    plugin: 'stepflow-builtins',
    type: 'builtin',
    description: 'OpenAI API integration for chat completions and embeddings',
    version: '1.0.0',
    status: 'active',
    usageCount: 245,
    schema: {
      input: ['prompt', 'model', 'temperature'],
      output: ['response', 'usage'],
    },
  },
  {
    name: 'eval',
    plugin: 'stepflow-builtins',
    type: 'builtin',
    description: 'Evaluate expressions and perform calculations',
    version: '1.0.0',
    status: 'active',
    usageCount: 156,
    schema: {
      input: ['expression', 'context'],
      output: ['result'],
    },
  },
  {
    name: 'load_file',
    plugin: 'stepflow-builtins',
    type: 'builtin',
    description: 'Load and read files from the filesystem',
    version: '1.0.0',
    status: 'active',
    usageCount: 89,
    schema: {
      input: ['path', 'encoding'],
      output: ['content', 'metadata'],
    },
  },
  {
    name: 'data_processor',
    plugin: 'python',
    type: 'external',
    description: 'Python-based data processing and transformation',
    version: '2.1.0',
    status: 'active',
    usageCount: 312,
    schema: {
      input: ['data', 'operation', 'parameters'],
      output: ['result', 'metrics'],
    },
  },
  {
    name: 'file_analyzer',
    plugin: 'python',
    type: 'external',
    description: 'Analyze and extract metadata from various file types',
    version: '1.5.2',
    status: 'active',
    usageCount: 67,
    schema: {
      input: ['file_path', 'analysis_type'],
      output: ['metadata', 'summary'],
    },
  },
  {
    name: 'notification_service',
    plugin: 'typescript',
    type: 'external',
    description: 'Send notifications via email, SMS, or webhooks',
    version: '0.9.0',
    status: 'maintenance',
    usageCount: 23,
    schema: {
      input: ['message', 'recipients', 'channel'],
      output: ['delivery_status'],
    },
  },
]

function getStatusBadge(status: string) {
  const variants: Record<string, { variant: 'default' | 'secondary' | 'destructive' | 'outline', color: string }> = {
    active: { variant: 'default', color: 'text-green-600' },
    maintenance: { variant: 'secondary', color: 'text-yellow-600' },
    inactive: { variant: 'outline', color: 'text-red-600' },
  }
  
  const config = variants[status] || variants.inactive
  return (
    <Badge variant={config.variant}>
      {status}
    </Badge>
  )
}

function getTypeIcon(type: string) {
  switch (type) {
    case 'builtin':
      return <Zap className="h-4 w-4 text-blue-500" />
    case 'external':
      return <Package className="h-4 w-4 text-green-500" />
    default:
      return <Package className="h-4 w-4 text-gray-500" />
  }
}

export default function ComponentsPage() {
  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">Components</h1>
          <p className="text-muted-foreground">
            Manage and monitor available workflow components
          </p>
        </div>
        <div className="flex items-center space-x-2">
          <Button variant="outline">
            <Settings className="mr-2 h-4 w-4" />
            Plugin Settings
          </Button>
          <Button>
            <Package className="mr-2 h-4 w-4" />
            Reload Components
          </Button>
        </div>
      </div>

      <div className="grid gap-4">
        {mockComponents.map((component) => (
          <Card key={`${component.plugin}-${component.name}`}>
            <CardHeader>
              <div className="flex items-center justify-between">
                <div className="flex items-center space-x-3">
                  {getTypeIcon(component.type)}
                  <div>
                    <CardTitle className="text-lg">{component.name}</CardTitle>
                    <CardDescription className="mt-1">
                      {component.description}
                    </CardDescription>
                  </div>
                  {getStatusBadge(component.status)}
                </div>
                <div className="flex items-center space-x-2">
                  <Button variant="ghost" size="sm">
                    <Info className="mr-2 h-4 w-4" />
                    Schema
                  </Button>
                  <Button variant="ghost" size="sm">
                    <ExternalLink className="h-4 w-4" />
                  </Button>
                </div>
              </div>
            </CardHeader>
            <CardContent>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm mb-4">
                <div>
                  <span className="text-muted-foreground">Plugin:</span>
                  <div className="font-medium">{component.plugin}</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Version:</span>
                  <div>{component.version}</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Type:</span>
                  <div className="capitalize">{component.type}</div>
                </div>
                <div>
                  <span className="text-muted-foreground">Usage Count:</span>
                  <div>{component.usageCount}</div>
                </div>
              </div>
              
              <div className="border rounded-lg p-3 bg-muted/50">
                <div className="text-sm font-medium mb-2">Input/Output Schema</div>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs">
                  <div>
                    <span className="text-muted-foreground">Input:</span>
                    <div className="mt-1 space-y-1">
                      {component.schema.input.map((field) => (
                        <Badge key={field} variant="outline" className="text-xs mr-1">
                          {field}
                        </Badge>
                      ))}
                    </div>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Output:</span>
                    <div className="mt-1 space-y-1">
                      {component.schema.output.map((field) => (
                        <Badge key={field} variant="outline" className="text-xs mr-1">
                          {field}
                        </Badge>
                      ))}
                    </div>
                  </div>
                </div>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>
    </div>
  )
}