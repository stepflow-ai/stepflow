import { NextRequest, NextResponse } from 'next/server'
import { getStepflowClient } from '@/lib/stepflow-client'
import { ErrorResponseSchema } from '@/lib/api-types'

// GET /api/components - List components (proxy to core server)
export async function GET(request: NextRequest) {
  try {
    const { searchParams } = new URL(request.url)
    const includeSchemas = searchParams.get('include_schemas') === 'true'

    const stepflowClient = getStepflowClient()
    const components = await stepflowClient.listComponents(includeSchemas)

    return NextResponse.json(components)
  } catch (error) {
    console.error('Failed to list components:', error)

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to list components',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}