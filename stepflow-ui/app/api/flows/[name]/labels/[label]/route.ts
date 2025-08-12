import { NextRequest, NextResponse } from 'next/server'
import { prisma } from '@/lib/db'
import { getStepFlowClient } from '@/lib/stepflow-client'
import {
  CreateLabelRequestSchema,
  LabelResponseSchema,
  ErrorResponseSchema,
  type LabelResponse,
} from '@/lib/api-types'

// POST /api/workflows/[name]/labels/[label] - Create or update a label
export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ name: string; label: string }> }
) {
  const resolvedParams = await params
  try {
    const workflowName = decodeURIComponent(resolvedParams.name)
    const labelName = decodeURIComponent(resolvedParams.label)
    const body = await request.json()
    const { flowId } = CreateLabelRequestSchema.parse(body)

    // Check if workflow exists
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

    // Verify the flow exists in core server
    const stepflowClient = getStepFlowClient()
    try {
      await stepflowClient.getFlow(flowId)
    } catch {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Flow not found',
        message: `Flow with hash ${flowId} not found in core server`,
        code: 404,
      })
      return NextResponse.json(errorResponse, { status: 404 })
    }

    // Create or update the label
    const label = await prisma.workflowLabel.upsert({
      where: {
        workflowName_label: {
          workflowName,
          label: labelName,
        },
      },
      update: {
        flowId,
        updatedAt: new Date(),
      },
      create: {
        workflowName,
        label: labelName,
        flowId,
      },
    })

    const response: LabelResponse = {
      workflowName: label.workflowName,
      label: label.label,
      flowId: label.flowId,
      createdAt: label.createdAt.toISOString(),
      updatedAt: label.updatedAt.toISOString(),
    }

    const validatedResponse = LabelResponseSchema.parse(response)
    return NextResponse.json(validatedResponse, { status: 201 })
  } catch (error) {
    console.error('Failed to create label:', error)

    if (error instanceof Error && error.name === 'ZodError') {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Invalid request',
        message: 'Request validation failed',
        code: 400,
      })
      return NextResponse.json(errorResponse, { status: 400 })
    }

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to create label',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}

// DELETE /api/workflows/[name]/labels/[label] - Delete a label
export async function DELETE(
  request: NextRequest,
  { params }: { params: Promise<{ name: string; label: string }> }
) {
  const resolvedParams = await params
  const workflowName = decodeURIComponent(resolvedParams.name)
  const labelName = decodeURIComponent(resolvedParams.label)
  try {

    // Delete the label
    await prisma.workflowLabel.delete({
      where: {
        workflowName_label: {
          workflowName,
          label: labelName,
        },
      },
    })

    return new NextResponse(null, { status: 204 })
  } catch (error) {
    console.error('Failed to delete label:', error)

    if (error instanceof Error && error.message.includes('Record to delete does not exist')) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Label not found',
        message: `No label '${labelName}' found for workflow '${workflowName}'`,
        code: 404,
      })
      return NextResponse.json(errorResponse, { status: 404 })
    }

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to delete label',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}