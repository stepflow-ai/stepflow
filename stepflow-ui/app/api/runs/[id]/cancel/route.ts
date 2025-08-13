import { NextRequest, NextResponse } from 'next/server'
import { getStepflowClient } from '@/lib/stepflow-client'
import { ErrorResponseSchema } from '@/lib/api-types'

// POST /api/runs/[id]/cancel - Cancel run (proxy to core server)
export async function POST(
  request: NextRequest,
  { params }: { params: Promise<{ id: string }> }
) {
  const resolvedParams = await params
  try {
    const runId = resolvedParams.id
    const stepflowClient = getStepflowClient()

    const result = await stepflowClient.cancelRun(runId)
    return NextResponse.json(result)
  } catch (error) {
    console.error('Failed to cancel run:', error)

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to cancel run',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}