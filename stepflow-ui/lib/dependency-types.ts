// Import and re-export the properly typed API client types
import type {
  FlowAnalysis,
  StepAnalysis,
  ValueDependencies,
  Dependency,
  DependencyOneOf1
} from '../stepflow-api-client/model'

export type {
  FlowAnalysis,
  StepAnalysis,
  ValueDependencies,
  Dependency
}

// Type for step dependency used by visualizer components
export interface StepDependency {
  stepId: string
  dependsOn: string[]
}

// Extract step IDs from a dependency array using TypeScript's discriminated union types
function extractStepIdsFromDependencies(dependencies: Dependency[]): string[] {
  return dependencies
    .filter((dep): dep is DependencyOneOf1 => dep && typeof dep === 'object' && 'stepOutput' in dep)
    .map(dep => dep.stepOutput.stepId)
}

// Simplified extraction function using properly typed API client interfaces
export function extractStepDependencies(stepAnalysis: StepAnalysis): string[] {
  if (!stepAnalysis?.inputDepends) {
    return []
  }

  const dependencies: string[] = []
  const inputDepends = stepAnalysis.inputDepends
  
  // Handle different dependency collection formats from the API
  if ('object' in inputDepends && inputDepends.object) {
    Object.values(inputDepends.object).forEach(collection => {
      if (collection instanceof Set) {
        dependencies.push(...extractStepIdsFromDependencies(Array.from(collection)))
      } else if (Array.isArray(collection)) {
        dependencies.push(...extractStepIdsFromDependencies(collection))
      }
    })
  }
  
  if ('other' in inputDepends && inputDepends.other) {
    if (inputDepends.other instanceof Set) {
      dependencies.push(...extractStepIdsFromDependencies(Array.from(inputDepends.other)))
    } else if (Array.isArray(inputDepends.other)) {
      dependencies.push(...extractStepIdsFromDependencies(inputDepends.other))
    }
  }

  return dependencies
}

// Unified type-safe function to convert analysis data to StepDependency format
export function convertAnalysisToDependencies(analysis: FlowAnalysis): StepDependency[] {
  if (!analysis?.steps) {
    return []
  }
  
  return Object.entries(analysis.steps)
    .map(([stepId, stepAnalysis]) => ({
      stepId,
      dependsOn: extractStepDependencies(stepAnalysis)
    }))
    .filter(dep => dep.dependsOn.length > 0)
}