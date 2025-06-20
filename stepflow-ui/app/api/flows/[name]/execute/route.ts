import { NextRequest, NextResponse } from 'next/server'
import { prisma } from '@/lib/db'
import { getStepFlowClient } from '@/lib/stepflow-client'
import {
  ExecuteWorkflowRequestSchema,
  ExecuteWorkflowResponseSchema,
  ErrorResponseSchema,
  type ExecuteWorkflowResponse,
} from '@/lib/api-types'

// POST /api/workflows/[name]/execute - Execute workflow by name
export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ name: string }> }
) {
  const resolvedParams = await params
  try {
    const workflowName = decodeURIComponent(resolvedParams.name)
    const body = await request.json()
    const { input, debug = false } = ExecuteWorkflowRequestSchema.parse(body)

    // Get workflow from database
    const workflow = await prisma.workflow.findUnique({
      where: { name: workflowName },
    })

    if (!workflow) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Workflow not found',
        message: `No workflow found with name: ${workflowName}`,
        code: 404,
      })
      return NextResponse.json(errorResponse, { status: 404 })
    }

    // Execute on core server using the latest flow hash
    const stepflowClient = getStepFlowClient()
    const runResult = await stepflowClient.createRun({
      flowHash: workflow.flowHash,
      input,
      debug,
    })

    // Store execution record in our database
    await prisma.workflowExecution.create({
      data: {
        id: runResult.runId,
        workflowName: workflowName,
        label: null, // Latest version has no label
        flowHash: workflow.flowHash,
        status: runResult.status,
        debug: runResult.debug,
        input: JSON.stringify(input),
        result: runResult.result ? JSON.stringify(runResult.result) : null,
        completedAt: runResult.status === 'completed' || runResult.status === 'failed' 
          ? new Date() 
          : null,
      },
    })

    const response: ExecuteWorkflowResponse = {
      runId: runResult.runId,
      result: runResult.result || undefined, // Convert null to undefined
      status: runResult.status,
      debug: runResult.debug,
      workflowName,
      flowHash: workflow.flowHash,
    }

    const validatedResponse = ExecuteWorkflowResponseSchema.parse(response)
    return NextResponse.json(validatedResponse)
  } catch (error) {
    console.error('Failed to execute workflow:', error)

    if (error instanceof Error && error.name === 'ZodError') {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Invalid request',
        message: 'Request validation failed',
        code: 400,
      })
      return NextResponse.json(errorResponse, { status: 400 })
    }

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to execute workflow',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}