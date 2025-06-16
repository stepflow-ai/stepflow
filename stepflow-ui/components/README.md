# Stepflow UI Components

This directory contains reusable UI components for the Stepflow application.

## WorkflowExecutionDialog Component

The `WorkflowExecutionDialog` component provides a shared, reusable dialog for executing workflows in different contexts throughout the application.

### Features

- **Workflow Execution Modes**: Can execute named workflows (latest or labeled versions) and ad-hoc workflow definitions
- **Input Examples**: Pre-configured examples with intelligent suggestions based on workflow components
- **Format Support**: JSON and YAML input formats with automatic conversion
- **Debug Mode**: Optional debug mode execution with step-by-step inspection
- **File Upload**: Support for uploading input files (extensible)
- **Input Validation**: Format validation and error handling
- **Responsive Design**: Mobile-friendly responsive layout

### Usage

#### Named Workflow Execution (Latest Version)
```tsx
import { WorkflowExecutionDialog } from '@/components/workflow-execution-dialog'

// Execute the latest version of a named workflow
<WorkflowExecutionDialog
  workflowName="data-pipeline"
  trigger={
    <Button>
      <Play className="mr-2 h-4 w-4" />
      Execute Latest
    </Button>
  }
/>
```

#### Labeled Workflow Execution
```tsx
import { WorkflowExecutionDialog } from '@/components/workflow-execution-dialog'

// Execute a specific labeled version
<WorkflowExecutionDialog
  workflowName="data-pipeline"
  workflowLabel="production"
  trigger={
    <Button>
      <Play className="mr-2 h-4 w-4" />
      Execute Production
    </Button>
  }
/>
```

#### Ad-hoc Workflow Execution
```tsx
import { WorkflowExecutionDialog } from '@/components/workflow-execution-dialog'

// Execute a workflow definition directly
<WorkflowExecutionDialog
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
import { WorkflowExecutionDialog } from '@/components/workflow-execution-dialog'

const [dialogOpen, setDialogOpen] = useState(false)

<WorkflowExecutionDialog
  workflow={workflow}
  open={dialogOpen}
  onOpenChange={setDialogOpen}
/>
```

### Props

| Prop | Type | Required | Description |
|------|------|----------|-------------|
| `workflowName` | `string` | No | Name of workflow to execute (for named mode) |
| `workflowLabel` | `string` | No | Label of workflow version (for labeled mode) |
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

The WorkflowExecutionDialog is integrated in several places:

1. **Workflows Page**: Quick execution from workflow cards
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