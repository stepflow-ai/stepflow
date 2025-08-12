import { NextRequest, NextResponse } from 'next/server'
import { prisma } from '@/lib/db'
import { getStepFlowClient } from '@/lib/stepflow-client'
import { ErrorResponseSchema } from '@/lib/api-types'

// GET /api/runs/[id]/workflow - Get workflow info for a run
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ id: string }> }
) {
  const resolvedParams = await params
  try {
    const runId = resolvedParams.id
    const stepflowClient = getStepFlowClient()
    
    // Get run details to get the flow ID
    const runDetails = await stepflowClient.getRun(runId)
    const flowId = runDetails.flowId
    
    // Try to find workflow in our database by flow ID
    const workflow = await prisma.workflow.findFirst({
      where: { flowId },
      include: {
        labels: {
          orderBy: { updatedAt: 'desc' },
        },
      },
    })
    
    // Get flow definition from core server
    const flowData = await stepflowClient.getFlow(flowId)
    
    const response = {
      // Run information
      runId: runDetails.runId,
      flowId: runDetails.flowId,
      debugMode: runDetails.debugMode,
      
      // Workflow metadata (may be null for ad-hoc workflows)
      workflowName: workflow?.name || runDetails.flowName || null,
      workflowDescription: workflow?.description || null,
      workflowLabels: workflow?.labels.map(label => ({
        label: label.label,
        flowId: label.flowId,
        createdAt: label.createdAt.toISOString(),
        updatedAt: label.updatedAt.toISOString(),
      })) || [],
      
      // Flow definition from core server
      flow: flowData.flow,
    }
    
    return NextResponse.json(response)
  } catch (error) {
    console.error('Failed to get run workflow:', error)

    if (error instanceof Error && error.message.includes('not found')) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Run or flow not found',
        message: 'Run or associated flow definition not found',
        code: 404,
      })
      return NextResponse.json(errorResponse, { status: 404 })
    }

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to get run workflow',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}