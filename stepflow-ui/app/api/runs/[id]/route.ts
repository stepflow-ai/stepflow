import { NextRequest, NextResponse } from 'next/server'
import { getStepFlowClient } from '@/lib/stepflow-client'
import { ErrorResponseSchema } from '@/lib/api-types'

// GET /api/runs/[id] - Get run details (proxy to core server)
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ id: string }> }
) {
  const resolvedParams = await params
  try {
    const runId = resolvedParams.id
    const stepflowClient = getStepFlowClient()
    
    const runDetails = await stepflowClient.getRun(runId)
    return NextResponse.json(runDetails)
  } catch (error) {
    console.error('Failed to get run details:', error)

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to get run details',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}

// DELETE /api/runs/[id] - Delete run (proxy to core server)
export async function DELETE(
  request: NextRequest,
  { params }: { params: Promise<{ id: string }> }
) {
  const resolvedParams = await params
  try {
    const runId = resolvedParams.id
    const stepflowClient = getStepFlowClient()
    
    await stepflowClient.deleteRun(runId)
    return new NextResponse(null, { status: 204 })
  } catch (error) {
    console.error('Failed to delete run:', error)

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to delete run',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}