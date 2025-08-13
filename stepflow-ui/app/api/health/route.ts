import { NextResponse } from 'next/server'
import { getStepflowClient } from '@/lib/stepflow-client'
import { ErrorResponseSchema } from '@/lib/api-types'

// GET /api/health - Health check (proxy to core server with UI status)
export async function GET() {
  try {
    const stepflowClient = getStepflowClient()

    // Check core server health
    let coreStatus = 'unknown'
    let coreVersion = 'unknown'

    try {
      const coreHealth = await stepflowClient.healthCheck()
      coreStatus = coreHealth.status || 'healthy'
      coreVersion = coreHealth.version || 'unknown'
    } catch (error) {
      console.warn('Core server health check failed:', error)
      coreStatus = 'unavailable'
    }

    // Return combined health status
    const healthResponse = {
      status: coreStatus === 'healthy' ? 'healthy' : 'degraded',
      timestamp: new Date().toISOString(),
      version: process.env.npm_package_version || '1.0.0',
      services: {
        ui: {
          status: 'healthy',
          version: process.env.npm_package_version || '1.0.0'
        },
        core: {
          status: coreStatus,
          version: coreVersion
        }
      }
    }

    // Return 200 for healthy, 503 for degraded
    const statusCode = healthResponse.status === 'healthy' ? 200 : 503

    return NextResponse.json(healthResponse, { status: statusCode })
  } catch (error) {
    console.error('Health check failed:', error)

    const errorResponse = ErrorResponseSchema.parse({
      error: 'Health check failed',
      message: error instanceof Error ? error.message : 'Unknown error',
      code: 500,
    })

    return NextResponse.json(errorResponse, { status: 500 })
  }
}