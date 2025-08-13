import { NextRequest, NextResponse } from 'next/server'
import { prisma } from '@/lib/db'
import { getStepflowClient } from '@/lib/stepflow-client'
import {
  ExecuteWorkflowRequestSchema,
  ExecuteWorkflowResponseSchema,
  ErrorResponseSchema,
  type ExecuteWorkflowResponse,
} from '@/lib/api-types'

// POST /api/workflows/[name]/labels/[label]/execute - Execute workflow by labeled version
export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ name: string; label: string }> }
) {
  const resolvedParams = await params
  try {
    const workflowName = decodeURIComponent(resolvedParams.name)
    const labelName = decodeURIComponent(resolvedParams.label)
    const body = await request.json()
    const { input, debug = false } = ExecuteWorkflowRequestSchema.parse(body)

    // Get the labeled workflow version
    const label = await prisma.workflowLabel.findUnique({
      where: {
        workflowName_label: {
          workflowName,
          label: labelName,
        },
      },
    })

    if (!label) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Label not found',
        message: `No label '${labelName}' found for workflow '${workflowName}'`,
        code: 404,
      })
      return NextResponse.json(errorResponse, { status: 404 })
    }

    // Execute on core server using the labeled flow ID
    const stepflowClient = getStepflowClient()
    const runResult = await stepflowClient.createRun({
      flowId: label.flowId, // flowId in DB maps to flowId in API
      input,
      debug,
    })

    // Store execution record in our database
    await prisma.workflowExecution.create({
      data: {
        id: runResult.runId,
        workflowName,
        label: labelName,
        flowId: label.flowId,
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
      flowId: label.flowId,
    }

    const validatedResponse = ExecuteWorkflowResponseSchema.parse(response)
    return NextResponse.json(validatedResponse)
  } catch (error) {
    console.error('Failed to execute labeled workflow:', error)

    if (error instanceof Error && error.name === 'ZodError') {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Invalid request',
        message: 'Request validation failed',
        code: 400,
      })
      return NextResponse.json(errorResponse, { status: 400 })
    }

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to execute labeled workflow',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}