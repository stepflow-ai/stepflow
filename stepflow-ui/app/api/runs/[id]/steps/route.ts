import { NextRequest, NextResponse } from 'next/server'
import { getStepflowClient } from '@/lib/stepflow-client'
import { ErrorResponseSchema } from '@/lib/api-types'

// GET /api/runs/[id]/steps - Get run step executions (proxy to core server)
export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ id: string }> }
) {
  const resolvedParams = await params
  try {
    const runId = resolvedParams.id
    const stepflowClient = getStepflowClient()

    const stepExecutions = await stepflowClient.getRunSteps(runId)
    return NextResponse.json(stepExecutions)
  } catch (error) {
    console.error('Failed to get run steps:', error)

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to get run steps',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}