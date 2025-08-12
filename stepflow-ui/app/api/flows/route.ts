import { NextRequest, NextResponse } from 'next/server'
import { prisma } from '@/lib/db'
import { getStepFlowClient } from '@/lib/stepflow-client'
import {
  StoreWorkflowRequestSchema,
  ListWorkflowsResponseSchema,
  ErrorResponseSchema,
  type WorkflowSummary,
} from '@/lib/api-types'
import type { AnalysisResult } from '@/stepflow-api-client'

// GET /api/flows - List all flows
export async function GET() {
  try {
    const workflows = await prisma.workflow.findMany({
      include: {
        labels: true,
        executions: {
          select: { id: true },
        },
      },
      orderBy: { updatedAt: 'desc' },
    })

    const workflowSummaries: WorkflowSummary[] = workflows.map(workflow => ({
      id: workflow.id,
      name: workflow.name,
      description: workflow.description,
      flowId: workflow.flowId,
      createdAt: workflow.createdAt.toISOString(),
      updatedAt: workflow.updatedAt.toISOString(),
      labelCount: workflow.labels.length,
      executionCount: workflow.executions.length,
    }))

    const response = ListWorkflowsResponseSchema.parse({
      workflows: workflowSummaries,
    })

    return NextResponse.json(response)
  } catch (error) {
    console.error('Failed to list flows:', error)
    
    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to list flows',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })
    
    return NextResponse.json(errorResponse, { status: 500 })
  }
}

// POST /api/flows - Store a new named flow
export async function POST(request: NextRequest) {
  try {
    const body = await request.json()
    const { name, flow, description } = StoreWorkflowRequestSchema.parse(body)

    const stepflowClient = getStepFlowClient()

    // Store the flow in the core server to get its hash
    const storeResult = await stepflowClient.storeFlow(flow) as AnalysisResult
    const flowId = storeResult.analysis?.flowId
    
    if (!flowId) {
      throw new Error('Failed to store flow: no flow ID returned')
    }

    // Store in our database with metadata
    const workflow = await prisma.workflow.upsert({
      where: { name },
      update: {
        description,
        flowId: flowId,
        updatedAt: new Date(),
      },
      create: {
        name,
        description,
        flowId: flowId,
      },
    })

    // Return a StoreFlowResponse that includes the original analysis result
    const response = {
      analysis: storeResult.analysis,
      diagnostics: storeResult.diagnostics || { diagnostics: [] },
      flowId: flowId,
      // Include workflow metadata for the UI
      workflow: {
        id: workflow.id,
        name: workflow.name,
        description: workflow.description,
        flowId: workflow.flowId,
        createdAt: workflow.createdAt.toISOString(),
        updatedAt: workflow.updatedAt.toISOString(),
      }
    }

    return NextResponse.json(response, { status: 201 })
  } catch (error) {
    console.error('Failed to store flow:', error)

    if (error instanceof Error && error.message.includes('Validation')) {
      const errorResponse = ErrorResponseSchema.parse({
        error: 'Invalid request',
        message: error.message,
        code: 400,
      })
      return NextResponse.json(errorResponse, { status: 400 })
    }

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Failed to store flow',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}