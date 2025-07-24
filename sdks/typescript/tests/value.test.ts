import { Value, StepReference, WorkflowInput, JsonPath } from '../src/value';

describe('JsonPath', () => {
  test('creates empty path defaulting to $', () => {
    const path = new JsonPath();
    expect(path.toString()).toBe('$');
  });

  test('adds fields and indices', () => {
    const path = new JsonPath();
    path.pushField('user').pushIndex('name').pushIndex(0);
    expect(path.toString()).toBe('$.user["name"][0]');
  });

  test('creates new paths with withField and withIndex', () => {
    const path = new JsonPath();
    const newPath = path.withField('data').withIndex('items').withIndex(1);
    
    expect(path.toString()).toBe('$');
    expect(newPath.toString()).toBe('$.data["items"][1]');
  });

  test('supports numeric indices', () => {
    const path = new JsonPath();
    const newPath = path.withIndex(0).withIndex(1).withIndex(2);
    expect(newPath.toString()).toBe('$[0][1][2]');
  });

  test('copies correctly', () => {
    const original = new JsonPath();
    original.pushField('test');
    
    const copy = original.copy();
    copy.pushField('copied');
    
    expect(original.toString()).toBe('$.test');
    expect(copy.toString()).toBe('$.test.copied');
  });
});

describe('StepReference', () => {
  test('creates basic step reference', () => {
    const ref = new StepReference('step1');
    expect(ref.stepId).toBe('step1');
    expect(ref.path.toString()).toBe('$');
  });

  test('supports bracket notation access', () => {
    const ref = new StepReference('step1');
    const fieldRef = ref.get('result');
    const indexRef = ref.get(0);
    
    expect(fieldRef.stepId).toBe('step1');
    expect(fieldRef.path.toString()).toBe('$["result"]');
    expect(indexRef.path.toString()).toBe('$[0]');
  });

  test('creates proxy for property access', () => {
    const ref = StepReference.create('step1');
    const fieldRef = (ref as any).status;
    const nestedRef = (ref as any).data.items;
    
    expect(fieldRef).toBeInstanceOf(StepReference);
    expect(fieldRef.stepId).toBe('step1');
    expect(fieldRef.path.toString()).toBe('$.status');
    
    expect(nestedRef).toBeInstanceOf(StepReference);
    expect(nestedRef.path.toString()).toBe('$.data.items');
  });

  test('converts to protocol reference', () => {
    const ref = new StepReference('step1');
    const protocolRef = ref.toReference();
    
    expect(protocolRef).toEqual({
      step: {
        step_id: 'step1',
        path: '$',
        on_skip: undefined
      }
    });
  });

  test('supports onSkip actions', () => {
    const skipAction = { type: 'default' as const, value: null };
    const ref = new StepReference('step1', undefined, skipAction);
    const newRef = ref.withOnSkip({ type: 'skip' as const });
    
    expect(ref.onSkip).toEqual(skipAction);
    expect(newRef.onSkip).toEqual({ type: 'skip' });
    expect(newRef.stepId).toBe('step1'); // Other properties preserved
  });
});

describe('WorkflowInput', () => {
  test('creates basic workflow input reference', () => {
    const input = new WorkflowInput();
    expect(input.path.toString()).toBe('$');
  });

  test('supports bracket notation access', () => {
    const input = new WorkflowInput();
    const fieldRef = input.get('config');
    const indexRef = input.get(0);
    
    expect(fieldRef.path.toString()).toBe('$["config"]');
    expect(indexRef.path.toString()).toBe('$[0]');
  });

  test('creates proxy for property access', () => {
    const input = WorkflowInput.create();
    const userRef = (input as any).user;
    const nameRef = (input as any).user.name;
    
    expect(userRef).toBeInstanceOf(WorkflowInput);
    expect(userRef.path.toString()).toBe('$.user');
    
    expect(nameRef).toBeInstanceOf(WorkflowInput);
    expect(nameRef.path.toString()).toBe('$.user.name');
  });

  test('converts to protocol reference', () => {
    const input = new WorkflowInput();
    const protocolRef = input.toReference();
    
    expect(protocolRef).toEqual({
      input: {
        path: '$',
        on_skip: undefined
      }
    });
  });

  test('supports onSkip actions', () => {
    const skipAction = { type: 'fail' as const };
    const input = new WorkflowInput(undefined, skipAction);
    const newInput = input.withOnSkip({ type: 'skip' as const });
    
    expect(input.onSkip).toEqual(skipAction);
    expect(newInput.onSkip).toEqual({ type: 'skip' });
  });
});

describe('Value', () => {
  test('creates literal values', () => {
    const stringLiteral = Value.literal('hello');
    const numberLiteral = Value.literal(42);
    const objectLiteral = Value.literal({ key: 'value' });
    
    const stringTemplate = stringLiteral.toValueTemplate();
    const numberTemplate = numberLiteral.toValueTemplate();
    const objectTemplate = objectLiteral.toValueTemplate();
    
    expect(stringTemplate).toEqual({ $literal: 'hello' });
    expect(numberTemplate).toEqual({ $literal: 42 });
    expect(objectTemplate).toEqual({ $literal: { key: 'value' } });
  });

  test('creates step references', () => {
    const stepRef = Value.step('step1');
    const stepRefWithPath = Value.step('step2', '$.result');
    
    const template1 = stepRef.toValueTemplate();
    const template2 = stepRefWithPath.toValueTemplate();
    
    expect(template1).toEqual({
      $from: {
        step: 'step1'
      }
    });
    
    expect(template2).toEqual({
      $from: {
        step: 'step2'
      },
      path: '$.result'
    });
  });

  test('creates workflow input references', () => {
    const inputRef = Value.input();
    const inputRefWithPath = Value.input('$.config.database');
    
    const template1 = Value.convertToValueTemplate(inputRef);
    const template2 = Value.convertToValueTemplate(inputRefWithPath);
    
    expect(template1).toEqual({
      $from: {
        workflow: 'input'
      }
    });
    
    expect(template2).toEqual({
      $from: {
        workflow: 'input'
      },
      path: '$.config.database'
    });
  });

  test('handles nested objects and arrays', () => {
    const complexValue = new Value({
      config: {
        database: Value.input().get('db_config'),
        retries: Value.literal(3)
      },
      steps: [
        Value.step('step1'),
        { id: Value.step('step2', '$.id') }
      ]
    });
    
    const template = complexValue.toValueTemplate();
    
    expect(template).toEqual({
      config: {
        database: {
          $from: {
            workflow: 'input'
          },
          path: '$["db_config"]'
        },
        retries: {
          $literal: 3
        }
      },
      steps: [
        {
          $from: {
            step: 'step1'
          }
        },
        {
          id: {
            $from: {
              step: 'step2'
            },
            path: '$.id'
          }
        }
      ]
    });
  });

  test('unwraps nested Value objects', () => {
    const innerValue = Value.literal('inner');
    const outerValue = new Value(innerValue);
    
    const template = outerValue.toValueTemplate();
    expect(template).toEqual({ $literal: 'inner' });
  });

  test('handles primitive types directly', () => {
    const stringValue = new Value('hello');
    const numberValue = new Value(42);
    const booleanValue = new Value(true);
    const nullValue = new Value(null);
    
    expect(stringValue.toValueTemplate()).toBe('hello');
    expect(numberValue.toValueTemplate()).toBe(42);
    expect(booleanValue.toValueTemplate()).toBe(true);
    expect(nullValue.toValueTemplate()).toBe(null);
  });

  test('converts StepReference and WorkflowInput objects', () => {
    const stepRef = new StepReference('step1');
    const inputRef = new WorkflowInput();
    
    const stepValue = new Value(stepRef);
    const inputValue = new Value(inputRef);
    
    expect(stepValue.toValueTemplate()).toEqual({
      $from: {
        step: 'step1'
      }
    });
    
    expect(inputValue.toValueTemplate()).toEqual({
      $from: {
        workflow: 'input'
      }
    });
  });

  test('handles arrays with mixed value types', () => {
    const arrayValue = new Value([
      'literal string',
      42,
      Value.literal('escaped literal'),
      Value.step('step1'),
      Value.input().get('config')
    ]);
    
    const template = arrayValue.toValueTemplate();
    
    expect(template).toEqual([
      'literal string',
      42,
      { $literal: 'escaped literal' },
      {
        $from: {
          step: 'step1'
        }
      },
      {
        $from: {
          workflow: 'input'
        },
        path: '$["config"]'
      }
    ]);
  });

  test('static convertToValueTemplate method', () => {
    const result = Value.convertToValueTemplate({
      literal: Value.literal('test'),
      step: new StepReference('step1'),
      input: new WorkflowInput(),
      nested: {
        array: [Value.step('step2'), 'plain string']
      }
    });
    
    expect(result).toEqual({
      literal: { $literal: 'test' },
      step: {
        $from: {
          step: 'step1'
        }
      },
      input: {
        $from: {
          workflow: 'input'
        }
      },
      nested: {
        array: [
          {
            $from: {
              step: 'step2'
            }
          },
          'plain string'
        ]
      }
    });
  });
});