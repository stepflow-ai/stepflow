import { NextRequest, NextResponse } from 'next/server'
import { prisma } from '@/lib/db'
import { getStepFlowClient } from '@/lib/stepflow-client'
import {
  WorkflowDetailSchema,
  ErrorResponseSchema,
  type WorkflowDetail,
  type StoreFlowResponse,
} from '@/lib/api-types'

// GET /api/flows/[name] - Get flow details with flow definition
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ name: string }> }
) {
  const resolvedParams = await params
  try {
    const workflowName = decodeURIComponent(resolvedParams.name)

    const workflow = await prisma.workflow.findUnique({
      where: { name: workflowName },
      include: {
        labels: {
          orderBy: { updatedAt: 'desc' },
        },
        executions: {
          select: {
            id: true,
            status: true,
            debug: true,
            createdAt: true,
            completedAt: true,
          },
          orderBy: { createdAt: 'desc' },
          take: 10, // Latest 10 executions
        },
      },
    })

    if (!workflow) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Flow not found',
        message: `No flow found with name: ${workflowName}`,
        code: 404,
      })
      return NextResponse.json(errorResponse, { status: 404 })
    }

    // Get the flow definition from core server
    const stepflowClient = getStepFlowClient()
    let flowData
    try {
      flowData = await stepflowClient.getFlow(workflow.flowId)
    } catch (error) {
      console.error(`Failed to fetch flow ${workflow.flowId} from core server:`, error)
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Flow definition not found',
        message: `Flow definition with hash ${workflow.flowId} not found in core server`,
        code: 422,
      })
      return NextResponse.json(errorResponse, { status: 422 })
    }

    const workflowDetail: WorkflowDetail = {
      id: workflow.id,
      name: workflow.name,
      description: workflow.description,
      flowId: workflow.flowId,
      flow: flowData.flow,
      analysis: flowData.analysis, // Include dependency analysis data
      createdAt: workflow.createdAt.toISOString(),
      updatedAt: workflow.updatedAt.toISOString(),
      labels: workflow.labels.map(label => ({
        label: label.label,
        flowId: label.flowId,
        createdAt: label.createdAt.toISOString(),
        updatedAt: label.updatedAt.toISOString(),
      })),
      recentExecutions: workflow.executions.map(execution => ({
        id: execution.id,
        status: execution.status as "running" | "completed" | "failed" | "paused" | "cancelled",
        debug: execution.debug,
        createdAt: execution.createdAt.toISOString(),
        completedAt: execution.completedAt?.toISOString() || null,
      })),
    }

    const response = WorkflowDetailSchema.parse(workflowDetail)
    return NextResponse.json(response)
  } catch (error) {
    console.error('Failed to get flow:', error)

    if (error instanceof Error && error.message.includes('not found')) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Flow not found in core server',
        message: 'Flow exists but flow definition not found in core server',
        code: 404,
      })
      return NextResponse.json(errorResponse, { status: 404 })
    }

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to get flow',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}

// PUT /api/flows/[name] - Update flow
export async function PUT(
  request: NextRequest,
  { params }: { params: Promise<{ name: string }> }
) {
  const resolvedParams = await params
  try {
    const workflowName = decodeURIComponent(resolvedParams.name)
    const body = await request.json()
    
    const { flow, description } = body

    // Store the updated flow in core server
    const stepflowClient = getStepFlowClient()
    const storeResult = await stepflowClient.storeFlow(flow) as StoreFlowResponse
    
    // Extract flow ID from the new response format
    const flowId = storeResult.flowId
    if (!flowId) {
      throw new Error('Failed to store flow: no flow ID returned')
    }

    // Update in our database
    const workflow = await prisma.workflow.update({
      where: { name: workflowName },
      data: {
        description,
        flowId,
        updatedAt: new Date(),
      },
    })

    const response = {
      id: workflow.id,
      name: workflow.name,
      description: workflow.description,
      flowId: workflow.flowId,
      createdAt: workflow.createdAt.toISOString(),
      updatedAt: workflow.updatedAt.toISOString(),
    }

    return NextResponse.json(response)
  } catch (error) {
    console.error('Failed to update flow:', error)

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to update flow',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}

// DELETE /api/flows/[name] - Delete flow
export async function DELETE(
  request: NextRequest,
  { params }: { params: Promise<{ name: string }> }
) {
  const resolvedParams = await params
  try {
    const workflowName = decodeURIComponent(resolvedParams.name)

    // Delete from our database (cascades to labels and executions)
    await prisma.workflow.delete({
      where: { name: workflowName },
    })

    return new NextResponse(null, { status: 204 })
  } catch (error) {
    console.error('Failed to delete flow:', error)

    if (error instanceof Error && error.message.includes('Record to delete does not exist')) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Flow not found',
        message: `No flow found with name: ${resolvedParams.name}`,
        code: 404,
      })
      return NextResponse.json(errorResponse, { status: 404 })
    }

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to delete flow',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}