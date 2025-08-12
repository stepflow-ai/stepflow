import {
  extractStepDependencies,
  convertAnalysisToDependencies,
  type FlowAnalysis,
  type StepAnalysis,
} from '@/lib/dependency-types'

describe('Dependency Extraction Functions', () => {
  describe('extractStepDependencies', () => {
    it('should extract dependencies from step analysis', () => {
      const stepAnalysis: StepAnalysis = {
        inputDepends: {
          object: {
            field1: new Set([
              { stepOutput: { stepId: 'step1', field: 'output', optional: false } },
              { stepOutput: { stepId: 'step2', field: 'result', optional: false } }
            ])
          }
        }
      }

      const dependencies = extractStepDependencies(stepAnalysis)
      expect(dependencies).toEqual(['step1', 'step2'])
    })

    it('should handle multiple object fields with dependencies', () => {
      const stepAnalysis: StepAnalysis = {
        inputDepends: {
          object: {
            field1: new Set([{ stepOutput: { stepId: 'step1', field: 'output', optional: false } }]),
            field2: new Set([{ stepOutput: { stepId: 'step2', field: 'result', optional: false } }])
          }
        }
      }

      const dependencies = extractStepDependencies(stepAnalysis)
      expect(dependencies).toEqual(['step1', 'step2'])
    })

    it('should handle other dependencies', () => {
      const stepAnalysis: StepAnalysis = {
        inputDepends: {
          other: new Set([
            { stepOutput: { stepId: 'step3', field: 'data', optional: false } }
          ])
        }
      }

      const dependencies = extractStepDependencies(stepAnalysis)
      expect(dependencies).toEqual(['step3'])
    })

    it('should return empty array for missing inputDepends', () => {
      const stepAnalysis: StepAnalysis = {
        inputDepends: { object: {} }
      }
      const dependencies = extractStepDependencies(stepAnalysis)
      expect(dependencies).toEqual([])
    })

    it('should handle mixed dependency types gracefully', () => {
      const stepAnalysis: StepAnalysis = {
        inputDepends: {
          object: {
            field1: new Set([
              { stepOutput: { stepId: 'step1', field: 'output', optional: false } },
              { flowInput: { field: 'inputField' } }, // Should be ignored
              'invalid dependency' as any // Should be ignored
            ])
          }
        }
      }

      const dependencies = extractStepDependencies(stepAnalysis)
      expect(dependencies).toEqual(['step1'])
    })
  })

  describe('convertAnalysisToDependencies', () => {
    it('should convert flow analysis to step dependencies', () => {
      const analysis: FlowAnalysis = {
        flow: {} as any,
        flowId: 'test-hash',
        outputDepends: {} as any,
        validationErrors: [],
        validationWarnings: [],
        steps: {
          step1: {
            inputDepends: {
              object: {
                field1: new Set([{ stepOutput: { stepId: 'input_step', field: 'data', optional: false } }])
              }
            }
          },
          step2: {
            inputDepends: {
              object: {
                field1: new Set([
                  { stepOutput: { stepId: 'step1', field: 'output', optional: false } },
                  { stepOutput: { stepId: 'input_step', field: 'config', optional: false } }
                ])
              }
            }
          },
          step3: {
            inputDepends: { object: {} }
          } // No dependencies
        }
      }

      const dependencies = convertAnalysisToDependencies(analysis)
      expect(dependencies).toEqual([
        { stepId: 'step1', dependsOn: ['input_step'] },
        { stepId: 'step2', dependsOn: ['step1', 'input_step'] }
      ])
    })

    it('should return empty array for missing steps', () => {
      const analysis: FlowAnalysis = {
        flow: {} as any,
        flowId: 'test-hash',
        outputDepends: {} as any,
        validationErrors: [],
        validationWarnings: [],
        steps: {}
      }
      const dependencies = convertAnalysisToDependencies(analysis)
      expect(dependencies).toEqual([])
    })

    it('should filter out steps with no dependencies', () => {
      const analysis: FlowAnalysis = {
        flow: {} as any,
        flowId: 'test-hash',
        outputDepends: {} as any,
        validationErrors: [],
        validationWarnings: [],
        steps: {
          step1: {
            inputDepends: { object: {} }
          }, // No dependencies
          step2: {
            inputDepends: {
              object: {
                field1: new Set([{ stepOutput: { stepId: 'step1', field: 'output', optional: false } }])
              }
            }
          }
        }
      }

      const dependencies = convertAnalysisToDependencies(analysis)
      expect(dependencies).toEqual([
        { stepId: 'step2', dependsOn: ['step1'] }
      ])
    })

    it('should handle complex dependency structures', () => {
      const analysis: FlowAnalysis = {
        flow: {} as any,
        flowId: 'test-hash',
        outputDepends: {} as any,
        validationErrors: [],
        validationWarnings: [],
        steps: {
          dataProcessor: {
            inputDepends: {
              object: {
                config: new Set([{ stepOutput: { stepId: 'configLoader', field: 'settings', optional: false } }]),
                data: new Set([{ stepOutput: { stepId: 'dataLoader', field: 'rawData', optional: false } }])
              },
              other: new Set([
                { stepOutput: { stepId: 'validator', field: 'schema', optional: false } }
              ])
            }
          }
        }
      }

      const dependencies = convertAnalysisToDependencies(analysis)
      expect(dependencies).toEqual([
        { stepId: 'dataProcessor', dependsOn: ['configLoader', 'dataLoader', 'validator'] }
      ])
    })
  })
})