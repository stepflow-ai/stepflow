import { NextRequest, NextResponse } from 'next/server'
import { getStepflowClient } from '@/lib/stepflow-client'
import { ErrorResponseSchema } from '@/lib/api-types'
import type { AnalysisResult } from '@/stepflow-api-client'

// POST /api/validate - Validate workflow without storing or executing
export async function POST(request: NextRequest) {
  try {
    const body = await request.json()
    const { workflow } = body

    if (!workflow) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Missing workflow',
        message: 'Workflow definition is required for validation',
        code: 400,
      })
      return NextResponse.json(errorResponse, { status: 400 })
    }

    const stepflowClient = getStepflowClient()

    // Use store flow to validate the workflow structure
    // This will return analysis results including any validation errors
    let storeResult
    try {
      storeResult = await stepflowClient.storeFlow(workflow)
    } catch (storeError) {
      console.error('Workflow validation failed:', storeError)

      // Extract error details if available
      let errorMessage = 'Invalid workflow definition'
      if (storeError instanceof Error) {
        errorMessage = storeError.message
      }

      return NextResponse.json({
        analysis: null,
        diagnostics: {
          diagnostics: [{
            level: 'error',
            message: errorMessage,
            location: null
          }]
        }
      }, { status: 200 }) // Return 200 with validation errors in diagnostics
    }

    const analysisResult = storeResult as AnalysisResult

    // Return the analysis and diagnostics
    return NextResponse.json({
      analysis: analysisResult.analysis,
      diagnostics: analysisResult.diagnostics || { diagnostics: [] }
    })

  } catch (error) {
    console.error('Failed to validate workflow:', error)

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Validation failed',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}