import { z } from 'zod';
import { workflowSchema, Workflow } from '../src/workflowSchema';

describe('Workflow Schema', () => {
  test('validates a simple workflow', () => {
    const workflow: Workflow = {
      input_schema: {
        type: 'object',
        properties: {
          name: { type: 'string' }
        },
        required: ['name']
      },
      steps: [
        {
          id: 'step1',
          component: 'http://example.com/component1',
          args: {
            input1: { literal: 'value' },
            input2: { input: 'name' }
          }
        }
      ]
    };

    const result = workflowSchema.safeParse(workflow);
    expect(result.success).toBe(true);
  });

  test('validates workflow with step references', () => {
    const workflow: Workflow = {
      steps: [
        {
          id: 'step1',
          component: 'http://example.com/component1',
          args: {
            input1: { literal: 'value' }
          }
        },
        {
          id: 'step2',
          component: 'http://example.com/component2',
          args: {
            input1: { step: 'step1', field: 'output' }
          }
        }
      ]
    };

    const result = workflowSchema.safeParse(workflow);
    expect(result.success).toBe(true);
  });

  test('rejects invalid workflow', () => {
    const invalidWorkflow = {
      steps: [
        {
          // Missing id
          component: 'http://example.com/component1',
          args: {}
        }
      ]
    };

    const result = workflowSchema.safeParse(invalidWorkflow);
    expect(result.success).toBe(false);
  });
});
