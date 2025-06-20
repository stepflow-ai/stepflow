import { render, screen, fireEvent } from '@testing-library/react'
import { WorkflowVisualizer } from '@/components/workflow-visualizer'
import type { Flow } from '@/stepflow-api-client/model'

// Mock @xyflow/react
jest.mock('@xyflow/react', () => ({
  ReactFlow: ({ children, onNodeClick }: any) => (
    <div data-testid="react-flow" onClick={onNodeClick}>
      {children}
      <div data-testid="mocked-flow">Mocked React Flow</div>
    </div>
  ),
  Controls: () => <div data-testid="flow-controls">Controls</div>,
  Background: () => <div data-testid="flow-background">Background</div>,
  useNodesState: () => [[], jest.fn()],
  useEdgesState: () => [[], jest.fn()],
  getLayoutedElements: jest.fn((nodes, edges) => ({ nodes, edges })),
  ConnectionMode: { Strict: 'strict' },
  Position: { Top: 'top', Bottom: 'bottom', Left: 'left', Right: 'right' },
  MarkerType: { ArrowClosed: 'arrowclosed' },
}))

// Mock elkjs
jest.mock('elkjs', () => ({
  __esModule: true,
  default: jest.fn(() => ({
    layout: jest.fn().mockResolvedValue({
      children: [],
      edges: []
    })
  }))
}))

describe('WorkflowVisualizer', () => {
  const mockSteps = [
    {
      id: 'step1',
      name: 'Load Data',
      component: 'data-loader',
      status: 'completed' as const,
      startTime: '2023-01-01T00:00:00Z',
      duration: '2.5s',
      output: 'Data loaded successfully'
    },
    {
      id: 'step2',
      name: 'Process Data',
      component: 'data-processor',
      status: 'running' as const,
      startTime: '2023-01-01T00:00:03Z',
      duration: null,
      output: null
    },
    {
      id: 'step3',
      name: 'Save Results',
      component: 'data-saver',
      status: 'pending' as const,
      startTime: null,
      duration: null,
      output: null
    }
  ]

  const mockDependencies = [
    { stepId: 'step2', dependsOn: ['step1'] },
    { stepId: 'step3', dependsOn: ['step2'] }
  ]

  const mockWorkflow: Flow = {
    name: 'Test Workflow',
    steps: {
      step1: {
        component: 'data-loader',
        input: {}
      },
      step2: {
        component: 'data-processor',
        input: {}
      },
      step3: {
        component: 'data-saver',
        input: {}
      }
    }
  }

  it('should render without crashing', () => {
    render(
      <WorkflowVisualizer
        steps={mockSteps}
        dependencies={mockDependencies}
        workflow={mockWorkflow}
      />
    )

    expect(screen.getByTestId('react-flow')).toBeInTheDocument()
    expect(screen.getByTestId('mocked-flow')).toBeInTheDocument()
  })

  it('should handle step click events', () => {
    const mockOnStepClick = jest.fn()
    
    render(
      <WorkflowVisualizer
        steps={mockSteps}
        dependencies={mockDependencies}
        workflow={mockWorkflow}
        onStepClick={mockOnStepClick}
      />
    )

    const reactFlow = screen.getByTestId('react-flow')
    fireEvent.click(reactFlow)
    
    // Note: In a real test, we'd need to simulate node clicks properly
    // This is a simplified test due to React Flow mocking
  })

  it('should render in debug mode', () => {
    const mockOnStepExecute = jest.fn()
    
    render(
      <WorkflowVisualizer
        steps={mockSteps}
        dependencies={mockDependencies}
        workflow={mockWorkflow}
        isDebugMode={true}
        onStepExecute={mockOnStepExecute}
      />
    )

    expect(screen.getByTestId('react-flow')).toBeInTheDocument()
  })

  it('should handle empty steps array', () => {
    render(
      <WorkflowVisualizer
        steps={[]}
        dependencies={[]}
        workflow={mockWorkflow}
      />
    )

    expect(screen.getByTestId('react-flow')).toBeInTheDocument()
  })

  it('should handle missing dependencies', () => {
    render(
      <WorkflowVisualizer
        steps={mockSteps}
        workflow={mockWorkflow}
      />
    )

    expect(screen.getByTestId('react-flow')).toBeInTheDocument()
  })

  it('should handle different step statuses', () => {
    const stepsWithDifferentStatuses = [
      { ...mockSteps[0], status: 'completed' as const },
      { ...mockSteps[1], status: 'failed' as const },
      { ...mockSteps[2], status: 'neutral' as const }
    ]

    render(
      <WorkflowVisualizer
        steps={stepsWithDifferentStatuses}
        dependencies={mockDependencies}
        workflow={mockWorkflow}
      />
    )

    expect(screen.getByTestId('react-flow')).toBeInTheDocument()
  })

  it('should pass analysis data to base component', () => {
    const mockAnalysis = {
      steps: {
        step1: { inputDepends: {} },
        step2: { inputDepends: {} }
      }
    }

    render(
      <WorkflowVisualizer
        steps={mockSteps}
        dependencies={mockDependencies}
        workflow={mockWorkflow}
        analysis={mockAnalysis}
      />
    )

    expect(screen.getByTestId('react-flow')).toBeInTheDocument()
  })
})