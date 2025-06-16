# Stepflow UI Components

This directory contains reusable UI components for the Stepflow application.

## ExecutionDialog Component

The `ExecutionDialog` component provides a shared, reusable dialog for executing workflows in different contexts throughout the application.

### Features

- **Dual Mode Support**: Can execute both endpoint-based workflows and ad-hoc workflow definitions
- **Input Examples**: Pre-configured examples with intelligent suggestions based on workflow components
- **Format Support**: JSON and YAML input formats with automatic conversion
- **Debug Mode**: Optional debug mode execution with step-by-step inspection
- **File Upload**: Support for uploading input files (extensible)
- **Input Validation**: Format validation and error handling
- **Responsive Design**: Mobile-friendly responsive layout

### Usage

#### Endpoint Execution
```tsx
import { ExecutionDialog } from '@/components/execution-dialog'

// Execute an endpoint with the dialog
<ExecutionDialog
  endpoint={endpointSummary}
  trigger={
    <Button>
      <Play className="mr-2 h-4 w-4" />
      Execute
    </Button>
  }
/>
```

#### Ad-hoc Workflow Execution
```tsx
import { ExecutionDialog } from '@/components/execution-dialog'

// Execute a workflow definition directly
<ExecutionDialog
  workflow={workflowDefinition}
  examples={customInputExamples}
  trigger={
    <Button>
      Quick Execute
    </Button>
  }
/>
```

#### Controlled Dialog
```tsx
import { ExecutionDialog } from '@/components/execution-dialog'

const [dialogOpen, setDialogOpen] = useState(false)

<ExecutionDialog
  workflow={workflow}
  open={dialogOpen}
  onOpenChange={setDialogOpen}
/>
```

### Props

| Prop | Type | Required | Description |
|------|------|----------|-------------|
| `endpoint` | `EndpointSummary` | No | Endpoint to execute (for endpoint mode) |
| `workflow` | `Workflow` | No | Workflow definition (for ad-hoc mode) |
| `trigger` | `React.ReactNode` | No | Custom trigger element |
| `open` | `boolean` | No | Controlled dialog state |
| `onOpenChange` | `(open: boolean) => void` | No | Controlled dialog state handler |
| `examples` | `InputExample[]` | No | Custom input examples |

### Input Examples

The component supports intelligent example suggestions:

1. **Automatic Detection**: Based on workflow components (e.g., Python math, OpenAI, file processing)
2. **Custom Examples**: Override with workflow-specific examples
3. **Common Examples**: Fallback to generic examples for unknown workflows

Examples are defined in `/lib/examples.ts` and include:
- Python Math Operations
- Text Processing
- OpenAI Chat Completions
- File Processing
- Data Pipeline Operations

### Integration Points

The ExecutionDialog is integrated in several places:

1. **Endpoints Page**: Quick execution from endpoint cards
2. **Execute Page**: Quick execute section with example workflows
3. **Components Page**: Could be extended for component testing
4. **Workflow Visualizer**: Could be added for testing workflows

### Example Workflows

The component includes several example workflows in `/lib/examples.ts`:

- **Python Math Operations**: Demonstrates arithmetic operations with Python components
- **Text Analysis Pipeline**: Comprehensive text processing with analysis steps
- **Expression Evaluation**: Simple expression evaluation using builtin eval component

### Styling

The component uses:
- **Tailwind CSS** for styling
- **Radix UI** primitives for accessibility
- **Lucide React** icons for visual elements
- **Responsive grid layouts** for mobile compatibility

### Future Enhancements

Potential improvements:

1. **Schema-based Input Generation**: Auto-generate input forms from workflow input schemas
2. **Real-time Validation**: Validate input against workflow schemas
3. **Input History**: Remember and suggest previously used inputs
4. **Workflow Templates**: Pre-configured workflow templates
5. **Collaborative Features**: Share and save execution configurations
6. **Advanced File Upload**: Support for CSV, XML, and other formats
7. **Input Transforms**: Built-in data transformation tools