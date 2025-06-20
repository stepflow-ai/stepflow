import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { WorkflowExecutionDialog } from '@/components/workflow-execution-dialog'
import * as useFlowApi from '@/lib/hooks/use-flow-api'

// Mock the hooks
jest.mock('@/lib/hooks/use-flow-api')
const mockUseFlowApi = useFlowApi as jest.Mocked<typeof useFlowApi>

// Mock the toast system
jest.mock('sonner', () => ({
  toast: {
    success: jest.fn(),
    error: jest.fn(),
  },
}))

const createWrapper = () => {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
    },
  })

  return ({ children }: { children: React.ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      {children}
    </QueryClientProvider>
  )
}

describe('WorkflowExecutionDialog', () => {
  const mockExecuteMutation = {
    mutate: jest.fn(),
    mutateAsync: jest.fn(),
    isPending: false,
    isError: false,
    isSuccess: false,
    isIdle: true,
    isPaused: false,
    error: null,
    data: null,
    reset: jest.fn(),
    status: 'idle' as const,
    variables: undefined,
    context: undefined,
    submittedAt: 0,
    failureCount: 0,
    failureReason: null,
  }

  const mockFlow = {
    name: 'test-workflow',
    description: 'A test workflow',
    inputSchema: {
      type: 'object',
      properties: {
        message: { type: 'string', description: 'Input message' },
        count: { type: 'number', description: 'Number of iterations' }
      },
      required: ['message']
    },
    steps: {
      step1: { component: 'test-component', input: {} }
    }
  }

  beforeEach(() => {
    jest.clearAllMocks()
    mockUseFlowApi.useExecuteFlow.mockReturnValue(mockExecuteMutation)
  })

  it('should render dialog when open', () => {
    const wrapper = createWrapper()
    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={jest.fn()}
        flow={mockFlow}
      />,
      { wrapper }
    )

    expect(screen.getByText('Execute Workflow')).toBeInTheDocument()
    expect(screen.getByText('test-workflow')).toBeInTheDocument()
    expect(screen.getByText('A test workflow')).toBeInTheDocument()
  })

  it('should not render dialog when closed', () => {
    const wrapper = createWrapper()
    render(
      <WorkflowExecutionDialog
        open={false}
        onClose={jest.fn()}
        flow={mockFlow}
      />,
      { wrapper }
    )

    expect(screen.queryByText('Execute Workflow')).not.toBeInTheDocument()
  })

  it('should render input form based on schema', () => {
    const wrapper = createWrapper()
    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={jest.fn()}
        flow={mockFlow}
      />,
      { wrapper }
    )

    expect(screen.getByLabelText(/message/i)).toBeInTheDocument()
    expect(screen.getByLabelText(/count/i)).toBeInTheDocument()
  })

  it('should handle form submission with valid input', async () => {
    const user = userEvent.setup()
    const mockOnClose = jest.fn()
    const wrapper = createWrapper()

    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={mockOnClose}
        flow={mockFlow}
      />,
      { wrapper }
    )

    // Fill out the form
    const messageInput = screen.getByLabelText(/message/i)
    const countInput = screen.getByLabelText(/count/i)
    
    await user.type(messageInput, 'Hello, World!')
    await user.type(countInput, '5')

    // Submit the form
    const executeButton = screen.getByRole('button', { name: /execute/i })
    await user.click(executeButton)

    expect(mockExecuteMutation.mutate).toHaveBeenCalledWith({
      flowHash: expect.any(String),
      input: {
        message: 'Hello, World!',
        count: 5
      }
    })
  })

  it('should show validation errors for required fields', async () => {
    const user = userEvent.setup()
    const wrapper = createWrapper()

    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={jest.fn()}
        flow={mockFlow}
      />,
      { wrapper }
    )

    // Try to submit without filling required fields
    const executeButton = screen.getByRole('button', { name: /execute/i })
    await user.click(executeButton)

    // Should show validation error (implementation would depend on form library)
    expect(mockExecuteMutation.mutate).not.toHaveBeenCalled()
  })

  it('should handle JSON input mode', async () => {
    const user = userEvent.setup()
    const wrapper = createWrapper()

    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={jest.fn()}
        flow={mockFlow}
      />,
      { wrapper }
    )

    // Switch to JSON mode
    const jsonToggle = screen.getByRole('button', { name: /json/i })
    await user.click(jsonToggle)

    // Should show JSON textarea
    const jsonInput = screen.getByRole('textbox', { name: /json input/i })
    expect(jsonInput).toBeInTheDocument()

    // Enter valid JSON
    await user.type(jsonInput, '{"message": "Hello from JSON", "count": 3}')

    // Submit
    const executeButton = screen.getByRole('button', { name: /execute/i })
    await user.click(executeButton)

    expect(mockExecuteMutation.mutate).toHaveBeenCalledWith({
      flowHash: expect.any(String),
      input: {
        message: 'Hello from JSON',
        count: 3
      }
    })
  })

  it('should show error for invalid JSON', async () => {
    const user = userEvent.setup()
    const wrapper = createWrapper()

    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={jest.fn()}
        flow={mockFlow}
      />,
      { wrapper }
    )

    // Switch to JSON mode
    const jsonToggle = screen.getByRole('button', { name: /json/i })
    await user.click(jsonToggle)

    // Enter invalid JSON
    const jsonInput = screen.getByRole('textbox', { name: /json input/i })
    await user.type(jsonInput, '{"invalid": json}')

    // Submit
    const executeButton = screen.getByRole('button', { name: /execute/i })
    await user.click(executeButton)

    // Should show error and not call mutate
    expect(screen.getByText(/invalid json/i)).toBeInTheDocument()
    expect(mockExecuteMutation.mutate).not.toHaveBeenCalled()
  })

  it('should show loading state during execution', () => {
    mockUseFlowApi.useExecuteFlow.mockReturnValue({
      ...mockExecuteMutation,
      isPending: true
    })

    const wrapper = createWrapper()
    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={jest.fn()}
        flow={mockFlow}
      />,
      { wrapper }
    )

    const executeButton = screen.getByRole('button', { name: /executing/i })
    expect(executeButton).toBeDisabled()
  })

  it('should close dialog on cancel', async () => {
    const user = userEvent.setup()
    const mockOnClose = jest.fn()
    const wrapper = createWrapper()

    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={mockOnClose}
        flow={mockFlow}
      />,
      { wrapper }
    )

    const cancelButton = screen.getByRole('button', { name: /cancel/i })
    await user.click(cancelButton)

    expect(mockOnClose).toHaveBeenCalled()
  })

  it('should close dialog on successful execution', async () => {
    const mockOnClose = jest.fn()
    
    // Mock successful execution
    mockUseFlowApi.useExecuteFlow.mockReturnValue({
      ...mockExecuteMutation,
      isSuccess: true,
      data: { 
        runId: 'run123', 
        status: 'running' as const,
        flowHash: 'hash123',
        debug: false,
        workflowName: 'test-workflow'
      }
    })

    const wrapper = createWrapper()
    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={mockOnClose}
        flow={mockFlow}
      />,
      { wrapper }
    )

    // The dialog should close automatically on success
    await waitFor(() => {
      expect(mockOnClose).toHaveBeenCalled()
    })
  })

  it('should handle execution errors', () => {
    const mockError = new Error('Execution failed')
    mockUseFlowApi.useExecuteFlow.mockReturnValue({
      ...mockExecuteMutation,
      isError: true,
      error: mockError
    })

    const wrapper = createWrapper()
    render(
      <WorkflowExecutionDialog
        open={true}
        onClose={jest.fn()}
        flow={mockFlow}
      />,
      { wrapper }
    )

    expect(screen.getByText(/execution failed/i)).toBeInTheDocument()
  })
})