import { NextRequest, NextResponse } from 'next/server'
import { prisma } from '@/lib/db'
import {
  ListLabelsResponseSchema,
  ErrorResponseSchema,
  type LabelResponse,
} from '@/lib/api-types'

// GET /api/workflows/[name]/labels - List all labels for a workflow
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ name: string }> }
) {
  const resolvedParams = await params
  try {
    const workflowName = decodeURIComponent(resolvedParams.name)

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

    // Get all labels for this workflow
    const labels = await prisma.workflowLabel.findMany({
      where: { workflowName },
      orderBy: { updatedAt: 'desc' },
    })

    const labelResponses: LabelResponse[] = labels.map(label => ({
      workflowName: label.workflowName,
      label: label.label,
      flowHash: label.flowHash,
      createdAt: label.createdAt.toISOString(),
      updatedAt: label.updatedAt.toISOString(),
    }))

    const response = ListLabelsResponseSchema.parse({
      labels: labelResponses,
    })

    return NextResponse.json(response)
  } catch (error) {
    console.error('Failed to list labels:', error)

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to list labels',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}