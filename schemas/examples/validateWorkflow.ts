import { workflowSchema } from '../src';

// Example workflow
const workflow = {
  input_schema: {
    type: 'object',
    properties: {
      name: { type: 'string' },
      age: { type: 'number' }
    },
    required: ['name']
  },
  steps: [
    {
      id: 'greet',
      component: 'http://example.com/greeter',
      args: {
        name: { input: 'name' },
        age: { input: 'age' },
        message: { literal: 'Hello, ' }
      }
    },
    {
      id: 'format',
      component: 'http://example.com/formatter',
      args: {
        text: { step: 'greet', field: 'greeting' },
        style: { literal: 'uppercase' }
      }
    }
  ],
  outputs: {
    greeting: { step: 'format', field: 'result' }
  }
};

// Validate the workflow
const result = workflowSchema.safeParse(workflow);

if (result.success) {
  console.log('Workflow is valid!');
  console.log(JSON.stringify(result.data, null, 2));
} else {
  console.error('Workflow validation failed:');
  console.error(JSON.stringify(result.error.format(), null, 2));
}
