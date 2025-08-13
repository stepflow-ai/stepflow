import { NextRequest, NextResponse } from 'next/server'
import { getStepflowClient } from '@/lib/stepflow-client'
import {
  ErrorResponseSchema,
  ExecuteAdHocWorkflowRequestSchema,
  ExecuteWorkflowResponseSchema,
} from '@/lib/api-types'
import type { AnalysisResult } from '@/stepflow-api-client'

// GET /api/runs - List all runs (proxy to core server)
export async function GET() {
  try {
    const stepflowClient = getStepflowClient()

    const runs = await stepflowClient.listRuns()
    return NextResponse.json(runs)
  } catch (error) {
    console.error('Failed to list runs:', error)

    // Check for connection errors
    if (error instanceof Error && error.message.includes('ECONNREFUSED')) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Stepflow server unavailable',
        message: 'Cannot connect to Stepflow core server. Please ensure the server is running on the configured URL.',
        code: 503, // Service Unavailable
      })
      return NextResponse.json(errorResponse, { status: 503 })
    }

    // Check for AggregateError (common with connection issues)
    if (error instanceof AggregateError) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Stepflow server unavailable',
        message: 'Cannot connect to Stepflow core server. Please ensure the server is running and accessible.',
        code: 503,
      })
      return NextResponse.json(errorResponse, { status: 503 })
    }

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to list runs',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}

// POST /api/runs - Execute ad-hoc workflow (without storing)
export async function POST(request: NextRequest) {
  try {
    const body = await request.json()
    const { workflow, input, debug = false } = ExecuteAdHocWorkflowRequestSchema.parse(body)

    const stepflowClient = getStepflowClient()

    // Store the workflow temporarily to get a hash, then execute it
    let storeResult
    try {
      storeResult = await stepflowClient.storeFlow(workflow)
    } catch (storeError) {
      console.error('Failed to store workflow for ad-hoc execution:', storeError)
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Invalid workflow definition',
        message: storeError instanceof Error ? storeError.message : 'Failed to store workflow',
        code: 400,
      })
      return NextResponse.json(errorResponse, { status: 400 })
    }

    const storeResponse = storeResult as AnalysisResult
    const flowId = storeResponse.analysis?.flowId

    if (!flowId) {
      throw new Error('Failed to store workflow: no flow ID returned')
    }

    // Execute the workflow directly by hash
    let runResult
    try {
      runResult = await stepflowClient.createRun({
        flowId,
        input,
        debug,
      })
    } catch (executeError) {
      console.error('Failed to execute workflow:', executeError)

      // Check for specific ad-hoc execution error
      if (executeError instanceof Error && executeError.message.includes('Ad-hoc flow execution not yet supported')) {
        const errorResponse = ErrorResponseSchema.parse({
          error: 'Ad-hoc execution not supported',
          message: 'Ad-hoc workflow execution is not yet supported by the Stepflow server. Please save the workflow first and then execute it.',
          code: 501, // Not Implemented
        })
        return NextResponse.json(errorResponse, { status: 501 })
      }

      const errorResponse = ErrorResponseSchema.parse({
        error: 'Execution failed',
        message: executeError instanceof Error ? executeError.message : 'Failed to execute workflow',
        code: 500,
      })
      return NextResponse.json(errorResponse, { status: 500 })
    }

    const response = ExecuteWorkflowResponseSchema.parse({
      runId: runResult.runId,
      status: runResult.status,
      flowId: flowId, // We have the flowId from the store operation
      debug: runResult.debug,
      workflowName: null, // Ad-hoc workflows don't have names
      result: runResult.result,
    })

    return NextResponse.json(response, { status: 201 })
  } catch (error) {
    console.error('Failed to execute ad-hoc workflow:', error)

    if (error instanceof Error && error.message.includes('Validation')) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Invalid request',
        message: error.message,
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