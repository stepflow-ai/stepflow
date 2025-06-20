import { z } from 'zod'

// ============================================================================
// Core StepFlow Types (mirrored from Rust)
// ============================================================================

export const FlowResultSchema = z.discriminatedUnion('outcome', [
  z.object({
    outcome: z.literal('success'),
    result: z.any(),
  }),
  z.object({
    outcome: z.literal('failed'),
    error: z.object({
      code: z.number(),
      message: z.string(),
    }),
  }),
  z.object({
    outcome: z.literal('skipped'),
  }),
])

export const ExecutionStatusSchema = z.enum([
  'running',
  'completed', 
  'failed',
  'paused',
  'cancelled'
])

// ============================================================================
// UI Server API Request/Response Types
// ============================================================================

// Workflow Management
export const StoreWorkflowRequestSchema = z.object({
  name: z.string().min(1),
  flow: z.any(), // The actual workflow definition
  description: z.string().optional(),
})

export const WorkflowSummarySchema = z.object({
  id: z.number(),
  name: z.string(),
  description: z.string().nullable(),
  flowHash: z.string(),
  createdAt: z.string(),
  updatedAt: z.string(),
  labelCount: z.number(),
  executionCount: z.number(),
})

export const WorkflowDetailSchema = z.object({
  id: z.number(),
  name: z.string(),
  description: z.string().nullable(),
  flowHash: z.string(),
  flow: z.any(), // The actual workflow definition from core server
  analysis: z.any().optional(), // FlowAnalysis from core server
  createdAt: z.string(),
  updatedAt: z.string(),
  labels: z.array(z.object({
    label: z.string(),
    flowHash: z.string(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })),
  recentExecutions: z.array(z.object({
    id: z.string(),
    status: ExecutionStatusSchema,
    debug: z.boolean(),
    createdAt: z.string(),
    completedAt: z.string().nullable(),
  })),
})

// Label Management
export const CreateLabelRequestSchema = z.object({
  flowHash: z.string(),
})

export const LabelResponseSchema = z.object({
  workflowName: z.string(),
  label: z.string(),
  flowHash: z.string(),
  createdAt: z.string(),
  updatedAt: z.string(),
})

// Execution
export const ExecuteWorkflowRequestSchema = z.object({
  input: z.record(z.unknown()),
  debug: z.boolean().optional().default(false),
})

// Ad-hoc Execution (execute workflow definition directly)
export const ExecuteAdHocWorkflowRequestSchema = z.object({
  workflow: z.object({}).passthrough(), // The workflow definition to execute (must be an object)
  input: z.record(z.unknown()),
  debug: z.boolean().optional().default(false),
})

export const ExecuteWorkflowResponseSchema = z.object({
  runId: z.string(),
  result: FlowResultSchema.optional(),
  status: ExecutionStatusSchema,
  debug: z.boolean(),
  workflowName: z.string().nullable(), // Allow null for ad-hoc workflows
  flowHash: z.string(),
})

// List Responses
export const ListWorkflowsResponseSchema = z.object({
  workflows: z.array(WorkflowSummarySchema),
})

export const ListLabelsResponseSchema = z.object({
  labels: z.array(LabelResponseSchema),
})

// Proxy Responses (from core server)
export const RunDetailsResponseSchema = z.object({
  runId: z.string(),
  flowName: z.string().nullable(),
  flowLabel: z.string().nullable(),
  flowHash: z.string(),
  status: ExecutionStatusSchema,
  debugMode: z.boolean(),
  createdAt: z.string(),
  completedAt: z.string().nullable(),
  input: z.record(z.unknown()),
  result: FlowResultSchema.optional(),
})

export const StepExecutionSchema = z.object({
  runId: z.string(),
  stepIndex: z.number(),
  stepId: z.string(),
  state: z.enum(['pending', 'running', 'completed', 'failed', 'skipped']),
  startedAt: z.string().nullable(),
  completedAt: z.string().nullable(),
  result: FlowResultSchema.optional(),
})

export const RunStepsResponseSchema = z.object({
  steps: z.array(StepExecutionSchema),
})

export const RunWorkflowResponseSchema = z.object({
  // Run information  
  runId: z.string(),
  flowHash: z.string(),
  debugMode: z.boolean(),
  
  // Workflow metadata (may be null for ad-hoc workflows)
  workflowName: z.string().nullable(),
  workflowDescription: z.string().nullable(),
  workflowLabels: z.array(z.object({
    label: z.string(),
    flowHash: z.string(),
    createdAt: z.string(),
    updatedAt: z.string(),
  })),
  
  // Flow definition from core server
  flow: z.any(),
})

export const ComponentResponseSchema = z.object({
  url: z.string(),
  name: z.string(),
  description: z.string().optional(),
  inputSchema: z.any().optional(),
  outputSchema: z.any().optional(),
})

export const ListComponentsResponseSchema = z.object({
  components: z.array(ComponentResponseSchema),
})

// ============================================================================
// Type Exports
// ============================================================================

export type FlowResult = z.infer<typeof FlowResultSchema>
export type ExecutionStatus = z.infer<typeof ExecutionStatusSchema>

// Workflow types
export type StoreWorkflowRequest = z.infer<typeof StoreWorkflowRequestSchema>
export type WorkflowSummary = z.infer<typeof WorkflowSummarySchema>
export type WorkflowDetail = z.infer<typeof WorkflowDetailSchema>

// Label types
export type CreateLabelRequest = z.infer<typeof CreateLabelRequestSchema>
export type LabelResponse = z.infer<typeof LabelResponseSchema>

// Execution types
export type ExecuteWorkflowRequest = z.infer<typeof ExecuteWorkflowRequestSchema>
export type ExecuteAdHocWorkflowRequest = z.infer<typeof ExecuteAdHocWorkflowRequestSchema>
export type ExecuteWorkflowResponse = z.infer<typeof ExecuteWorkflowResponseSchema>

// List types
export type ListWorkflowsResponse = z.infer<typeof ListWorkflowsResponseSchema>
export type ListLabelsResponse = z.infer<typeof ListLabelsResponseSchema>

// Proxy types
export type RunDetailsResponse = z.infer<typeof RunDetailsResponseSchema>
export type StepExecution = z.infer<typeof StepExecutionSchema>
export type RunStepsResponse = z.infer<typeof RunStepsResponseSchema>
export type RunWorkflowResponse = z.infer<typeof RunWorkflowResponseSchema>
export type ComponentResponse = z.infer<typeof ComponentResponseSchema>
export type ListComponentsResponse = z.infer<typeof ListComponentsResponseSchema>

// ============================================================================
// Error Response
// ============================================================================

export const ErrorResponseSchema = z.object({
  error: z.string(),
  message: z.string().optional(),
  code: z.number().optional(),
})

export type ErrorResponse = z.infer<typeof ErrorResponseSchema>