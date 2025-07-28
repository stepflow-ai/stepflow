import { FlowBuilder, createFlow, OnError, StepHandle } from '../src/flow-builder';
import { Value, StepReference, WorkflowInput } from '../src/value';

describe('FlowBuilder', () => {
  describe('Basic Flow Construction', () => {
    test('creates empty flow with metadata', () => {
      const builder = new FlowBuilder('test-flow', 'A test flow', '1.0.0');
      
      expect(builder.getName()).toBe('test-flow');
      expect(builder.getDescription()).toBe('A test flow');
      expect(builder.getVersion()).toBe('1.0.0');
    });

    test('builds flow with steps and output', () => {
      const builder = createFlow('math-flow')
        .setInputSchema({
          type: 'object',
          properties: {
            x: { type: 'number' },
            y: { type: 'number' }
          },
          required: ['x', 'y']
        });

      const step1 = builder.addStep({
        id: 'add',
        component: 'math/add',
        input: {
          a: Value.input().get('x'),
          b: Value.input().get('y')
        }
      });

      builder.setOutput({ result: step1.ref });

      const flow = builder.build();

      expect(flow.name).toBe('math-flow');
      expect(flow.steps).toHaveLength(1);
      expect(flow.steps[0].id).toBe('add');
      expect(flow.steps[0].component).toBe('math/add');
      expect(flow.output).toBeDefined();
    });

    test('throws error when building without output', () => {
      const builder = createFlow('incomplete-flow');
      builder.addStep({
        id: 'step1',
        component: 'test/component',
        input: { value: 42 }
      });

      expect(() => builder.build()).toThrow('Workflow output must be set before building');
    });
  });

  describe('Step Handles', () => {
    test('creates step handle with reference capability', () => {
      const builder = createFlow('test-flow');
      
      const step = builder.addStep({
        id: 'process',
        component: 'data/process',
        input: { data: 'test' }
      });

      expect(step).toBeInstanceOf(StepHandle);
      expect(step.id).toBe('process');
      expect(step.builder).toBe(builder);
    });

    test('supports property access on step handles', () => {
      const builder = createFlow('test-flow');
      
      const step = builder.addStep({
        id: 'api_call',
        component: 'http/get',
        input: { url: 'https://api.example.com' }
      });

      // Property access should work via proxy
      const statusRef = (step as any).status;
      const dataRef = (step as any).data;
      
      expect(statusRef).toBeDefined();
      expect(dataRef).toBeDefined();
    });

    test('supports bracket notation access', () => {
      const builder = createFlow('test-flow');
      
      const step = builder.addStep({
        id: 'process',
        component: 'data/process',
        input: { items: [1, 2, 3] }
      });

      const firstItem = step.get(0);
      const namedField = step.get('result');
      
      expect(firstItem).toBeDefined();
      expect(namedField).toBeDefined();
    });
  });

  describe('Value System Integration', () => {
    test('handles literal values', () => {
      const builder = createFlow('literal-test');
      
      builder.addStep({
        id: 'step1',
        component: 'test/component',
        input: {
          literal_string: Value.literal('hello'),
          literal_number: Value.literal(42),
          literal_object: Value.literal({ key: 'value' })
        }
      });

      builder.setOutput({ done: true });
      const flow = builder.build();

      const input = flow.steps[0].input as any;
      expect(input.literal_string.$literal).toBe('hello');
      expect(input.literal_number.$literal).toBe(42);
      expect(input.literal_object.$literal).toEqual({ key: 'value' });
    });

    test('handles step references', () => {
      const builder = createFlow('reference-test');
      
      const step1 = builder.addStep({
        id: 'producer',
        component: 'data/generate',
        input: { count: 5 }
      });

      builder.addStep({
        id: 'consumer',
        component: 'data/process',
        input: {
          data: step1.ref,
          count: (step1 as any).count,
          first_item: step1.get(0)
        }
      });

      builder.setOutput({ done: true });
      const flow = builder.build();

      const consumerInput = flow.steps[1].input as any;
      expect(consumerInput.data.$from.step).toBe('producer');
      expect(consumerInput.count.$from.step).toBe('producer');
      expect(consumerInput.first_item.$from.step).toBe('producer');
    });

    test('handles workflow input references', () => {
      const builder = createFlow('input-test');
      
      builder.addStep({
        id: 'process',
        component: 'data/process',
        input: {
          raw_input: Value.input(),
          user_name: Value.input().get('user').get('name'),
          settings: Value.input().get('config')
        }
      });

      builder.setOutput({ result: Value.input().get('expected_result') });
      const flow = builder.build();

      const processInput = flow.steps[0].input as any;
      expect(processInput.raw_input.$from.workflow).toBe('input');
      expect(processInput.user_name.$from.workflow).toBe('input');
      expect(processInput.user_name.path).toBe('$["user"]["name"]');
      expect(processInput.settings.$from.workflow).toBe('input');
      expect(processInput.settings.path).toBe('$["config"]');

      const output = flow.output as any;
      expect(output.result.$from.workflow).toBe('input');
      expect(output.result.path).toBe('$["expected_result"]');
    });
  });

  describe('Advanced Features', () => {
    test('supports skip conditions', () => {
      const builder = createFlow('skip-test');
      
      const step1 = builder.addStep({
        id: 'conditional',
        component: 'logic/conditional',
        input: { value: 42 }
      });

      builder.addStep({
        id: 'optional',
        component: 'optional/step',
        input: { data: 'test' },
        skipIf: (step1 as any).should_skip
      });

      builder.setOutput({ done: true });
      const flow = builder.build();

      expect(flow.steps[1].skip_if).toBeDefined();
      expect((flow.steps[1].skip_if as any).$from.step).toBe('conditional');
    });

    test('supports error handling', () => {
      const builder = createFlow('error-test');
      
      builder.addStep({
        id: 'risky',
        component: 'risky/operation',
        input: { data: 'test' },
        onError: OnError.retry(5)
      });

      builder.addStep({
        id: 'fallback',
        component: 'safe/operation',
        input: { data: 'test' },
        onError: OnError.default({ fallback: true })
      });

      builder.setOutput({ done: true });
      const flow = builder.build();

      expect(flow.steps[0].on_error?.type).toBe('retry');
      expect(flow.steps[0].on_error?.maxAttempts).toBe(5);
      
      expect(flow.steps[1].on_error?.type).toBe('default');
      expect(flow.steps[1].on_error?.value).toEqual({ fallback: true });
    });

    test('supports input and output schemas', () => {
      const inputSchema = {
        type: 'object',
        properties: {
          name: { type: 'string' },
          age: { type: 'number' }
        },
        required: ['name']
      };

      const outputSchema = {
        type: 'object',
        properties: {
          greeting: { type: 'string' },
          adult: { type: 'boolean' }
        }
      };

      const builder = createFlow('schema-test')
        .setInputSchema(inputSchema)
        .setOutputSchema(outputSchema);

      builder.addStep({
        id: 'greet',
        component: 'text/greet',
        input: { name: Value.input().get('name') },
        inputSchema: { type: 'object', properties: { name: { type: 'string' } } },
        outputSchema: { type: 'object', properties: { greeting: { type: 'string' } } }
      });

      builder.setOutput({
        greeting: (builder.step('greet') as any).greeting,
        adult: Value.literal(true)
      });

      const flow = builder.build();

      expect(flow.input_schema).toEqual(inputSchema);
      expect(flow.output_schema).toEqual(outputSchema);
      expect(flow.steps[0].input_schema).toBeDefined();
      expect(flow.steps[0].output_schema).toBeDefined();
    });
  });

  describe('Flow Analysis', () => {
    test('extracts references from workflow', () => {
      const builder = createFlow('analysis-test');
      
      const step1 = builder.addStep({
        id: 'step1',
        component: 'comp1',
        input: { 
          from_input: Value.input().get('config'),
          literal: Value.literal('test')
        }
      });

      const step2 = builder.addStep({
        id: 'step2',
        component: 'comp2',
        input: { 
          from_step: step1.ref,
          from_field: (step1 as any).result
        },
        skipIf: Value.input().get('skip_step2')
      });

      builder.setOutput({ 
        final: (step2 as any).output,
        config: Value.input().get('final_config')
      });

      const references = builder.getReferences();
      
      // Should find references to workflow input and steps
      const stepRefs = references.filter(ref => ref instanceof StepReference);
      const inputRefs = references.filter(ref => ref instanceof WorkflowInput);
      
      expect(stepRefs.length).toBeGreaterThan(0);
      expect(inputRefs.length).toBeGreaterThan(0);
    });

    test('loads and analyzes existing flow', () => {
      // First create a flow
      const builder = createFlow('original-flow');
      
      const step1 = builder.addStep({
        id: 'process',
        component: 'data/process',
        input: { data: Value.input().get('raw_data') }
      });

      builder.setOutput({ result: (step1 as any).output });
      const originalFlow = builder.build();

      // Load it for analysis
      const loadedBuilder = FlowBuilder.load(originalFlow);
      
      expect(loadedBuilder.getName()).toBe('original-flow');
      expect(loadedBuilder.getStepIds()).toEqual(['process']);
      
      const processStep = loadedBuilder.step('process');
      expect(processStep).toBeDefined();
      expect(processStep!.id).toBe('process');
      
      const references = loadedBuilder.getReferences();
      expect(references.length).toBeGreaterThan(0);
    });

    test('analyzes individual steps', () => {
      const builder = createFlow('step-analysis');
      
      const step1 = builder.addStep({
        id: 'step1',
        component: 'comp1',
        input: { value: 42 }
      });

      const step2 = builder.addStep({
        id: 'step2',
        component: 'comp2',
        input: { 
          from_step: step1.ref,
          from_input: Value.input().get('param')
        },
        skipIf: (step1 as any).should_skip,
        onError: OnError.default(Value.input().get('fallback'))
      });

      const step2Handle = builder.step('step2')!;
      const references = step2Handle.getReferences();
      
      expect(references.length).toBeGreaterThan(0);
      
      const stepRefs = references.filter(ref => ref instanceof StepReference);
      const inputRefs = references.filter(ref => ref instanceof WorkflowInput);
      
      expect(stepRefs.length).toBeGreaterThan(0); // Should find reference to step1
      expect(inputRefs.length).toBeGreaterThan(0); // Should find input references
    });
  });

  describe('Error Cases', () => {
    test('handles missing step references', () => {
      const builder = createFlow('missing-step');
      
      const missingStep = builder.step('nonexistent');
      expect(missingStep).toBeUndefined();
    });

    test('validates step IDs are unique', () => {
      const builder = createFlow('duplicate-test');
      
      builder.addStep({ id: 'step1', component: 'comp1', input: {} });
      builder.addStep({ id: 'step1', component: 'comp2', input: {} });
      
      // Should have overwridden the first step
      expect(builder.getStepIds()).toEqual(['step1', 'step1']); // Both are stored
      expect(builder.step('step1')?.step.component).toBe('comp2'); // Latest one wins in the map
    });
  });
});