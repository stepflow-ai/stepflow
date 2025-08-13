/**
 * Flow Builder API for programmatically creating Stepflow workflows.
 * This provides a TypeScript implementation of the Python SDK's FlowBuilder.
 */

import { Value, StepReference, WorkflowInput, Valuable, SkipAction, JsonPath } from './value';
import { ValueTemplate } from './protocol';

export interface Schema {
  type?: string;
  properties?: Record<string, any>;
  required?: string[];
  [key: string]: any;
}

export interface OnErrorAction {
  type: 'fail' | 'skip' | 'retry' | 'default';
  value?: any;
  maxAttempts?: number;
}

export interface Step {
  id: string;
  component: string;
  input?: ValueTemplate;
  input_schema?: Schema;
  output_schema?: Schema;
  skip_if?: ValueTemplate;
  on_error?: OnErrorAction;
}

export interface Flow {
  name?: string;
  description?: string;
  version?: string;
  input_schema?: Schema;
  output_schema?: Schema;
  steps: Step[];
  output?: ValueTemplate;
}

/**
 * A handle for interacting with steps, enabling reference creation and analysis.
 */
export class StepHandle {
  private _stepReference: StepReference;

  constructor(
    public readonly id: string,
    public readonly step: Step,
    public readonly builder: FlowBuilder
  ) {
    this._stepReference = StepReference.create(id);
  }

  /**
   * Extract all references used in this step.
   */
  getReferences(): Array<StepReference | WorkflowInput> {
    const references: Array<StepReference | WorkflowInput> = [];

    const extractFromValueTemplate = (template: ValueTemplate | undefined): void => {
      if (!template) return;

      if (typeof template === 'object' && template !== null) {
        if ('$from' in template && template.$from) {
          const ref = template.$from as any;
          if ('step' in ref) {
            const path = 'path' in template ? template.path as string : '$';
            const onSkip = 'onSkip' in template ? template.onSkip as any : undefined;
            references.push(new StepReference(ref.step, new JsonPath([path]), onSkip));
          } else if ('workflow' in ref && ref.workflow === 'input') {
            const path = 'path' in template ? template.path as string : '$';
            const onSkip = 'onSkip' in template ? template.onSkip as any : undefined;
            references.push(new WorkflowInput(new JsonPath([path]), onSkip));
          }
        }

        if (Array.isArray(template)) {
          template.forEach(item => extractFromValueTemplate(item));
        } else {
          Object.values(template).forEach(value => extractFromValueTemplate(value));
        }
      }
    };

    extractFromValueTemplate(this.step.input);
    extractFromValueTemplate(this.step.skip_if);

    if (this.step.on_error?.type === 'default' && this.step.on_error.value) {
      extractFromValueTemplate(Value.convertToValueTemplate(this.step.on_error.value));
    }

    return references;
  }

  /**
   * Create a reference to this step's output using bracket notation.
   */
  get(key: string | number): StepReference {
    return this._stepReference.get(key);
  }

  /**
   * Get the underlying StepReference for property access.
   */
  get ref(): StepReference {
    return this._stepReference;
  }

  /**
   * Create a reference using property access via proxy.
   */
  [key: string]: any;
}

// Create proxy for StepHandle to enable property access
function createStepHandleProxy(handle: StepHandle): StepHandle {
  return new Proxy(handle, {
    get(target, prop) {
      // First check if the property exists on the target itself
      if (prop in target || typeof prop === 'symbol') {
        return (target as any)[prop];
      }

      // For string properties that don't exist on target and don't start with underscore
      if (typeof prop === 'string' && !prop.startsWith('_')) {
        // Delegate to the underlying StepReference
        return (target.ref as any)[prop];
      }

      return (target as any)[prop];
    }
  });
}

/**
 * The main class for programmatically building workflows.
 */
export class FlowBuilder {
  private _name?: string;
  private _description?: string;
  private _version?: string;
  private _inputSchema?: Schema;
  private _outputSchema?: Schema;
  private _steps: Step[] = [];
  private _stepHandles: Map<string, StepHandle> = new Map();
  private _output?: ValueTemplate;

  constructor(name?: string, description?: string, version?: string) {
    this._name = name;
    this._description = description;
    this._version = version;
  }

  /**
   * Set the input schema for the workflow.
   */
  setInputSchema(schema: Schema): FlowBuilder {
    this._inputSchema = schema;
    return this;
  }

  /**
   * Set the output schema for the workflow.
   */
  setOutputSchema(schema: Schema): FlowBuilder {
    this._outputSchema = schema;
    return this;
  }

  /**
   * Set the workflow output (required before building).
   */
  setOutput(output: Valuable): FlowBuilder {
    this._output = Value.convertToValueTemplate(output);
    return this;
  }

  /**
   * Add a step to the workflow.
   */
  addStep(options: {
    id: string;
    component: string;
    input?: Valuable;
    inputSchema?: Schema;
    outputSchema?: Schema;
    skipIf?: Valuable;
    onError?: OnErrorAction;
  }): StepHandle {
    const step: Step = {
      id: options.id,
      component: options.component,
      input: options.input ? Value.convertToValueTemplate(options.input) : undefined,
      input_schema: options.inputSchema,
      output_schema: options.outputSchema,
      skip_if: options.skipIf ? Value.convertToValueTemplate(options.skipIf) : undefined,
      on_error: options.onError
    };

    this._steps.push(step);
    const handle = createStepHandleProxy(new StepHandle(options.id, step, this));
    this._stepHandles.set(options.id, handle);

    return handle;
  }

  /**
   * Get a StepHandle by step ID for analysis.
   */
  step(stepId: string): StepHandle | undefined {
    return this._stepHandles.get(stepId);
  }

  /**
   * Extract all references used in the workflow.
   */
  getReferences(): Array<StepReference | WorkflowInput> {
    const references: Array<StepReference | WorkflowInput> = [];

    // Extract references from all steps
    this._stepHandles.forEach(handle => {
      references.push(...handle.getReferences());
    });

    // Extract references from workflow output
    if (this._output) {
      const extractFromValueTemplate = (template: ValueTemplate): void => {
        if (typeof template === 'object' && template !== null) {
          if ('$from' in template && template.$from) {
            const ref = template.$from as any;
            if ('step' in ref) {
              const path = 'path' in template ? template.path as string : '$';
              const onSkip = 'onSkip' in template ? template.onSkip as any : undefined;
              references.push(new StepReference(ref.step, new JsonPath([path]), onSkip));
            } else if ('workflow' in ref && ref.workflow === 'input') {
              const path = 'path' in template ? template.path as string : '$';
              const onSkip = 'onSkip' in template ? template.onSkip as any : undefined;
              references.push(new WorkflowInput(new JsonPath([path]), onSkip));
            }
          }

          if (Array.isArray(template)) {
            template.forEach(item => extractFromValueTemplate(item));
          } else {
            Object.values(template).forEach(value => {
              if (value && typeof value === 'object') {
                extractFromValueTemplate(value);
              }
            });
          }
        }
      };

      extractFromValueTemplate(this._output);
    }

    return references;
  }

  /**
   * Build and return the Flow object.
   */
  build(): Flow {
    if (!this._output) {
      throw new Error('Workflow output must be set before building. Use setOutput() to specify the output.');
    }

    return {
      name: this._name,
      description: this._description,
      version: this._version,
      input_schema: this._inputSchema,
      output_schema: this._outputSchema,
      steps: [...this._steps],
      output: this._output
    };
  }

  /**
   * Create a FlowBuilder from an existing Flow for analysis.
   */
  static load(flow: Flow): FlowBuilder {
    const builder = new FlowBuilder(flow.name, flow.description, flow.version);

    builder._inputSchema = flow.input_schema;
    builder._outputSchema = flow.output_schema;
    builder._output = flow.output;

    // Recreate steps and handles
    flow.steps.forEach(step => {
      builder._steps.push({ ...step });
      const handle = createStepHandleProxy(new StepHandle(step.id, step, builder));
      builder._stepHandles.set(step.id, handle);
    });

    return builder;
  }

  /**
   * Get all step IDs in the workflow.
   */
  getStepIds(): string[] {
    return this._steps.map(step => step.id);
  }

  /**
   * Get the workflow name.
   */
  getName(): string | undefined {
    return this._name;
  }

  /**
   * Get the workflow description.
   */
  getDescription(): string | undefined {
    return this._description;
  }

  /**
   * Get the workflow version.
   */
  getVersion(): string | undefined {
    return this._version;
  }
}

/**
 * Error action helpers for creating OnErrorAction objects.
 */
export class OnError {
  static fail(): OnErrorAction {
    return { type: 'fail' };
  }

  static skip(): OnErrorAction {
    return { type: 'skip' };
  }

  static retry(maxAttempts: number = 3): OnErrorAction {
    return { type: 'retry', maxAttempts };
  }

  static default(value: any): OnErrorAction {
    return { type: 'default', value };
  }
}

/**
 * Convenience function to create a new FlowBuilder.
 */
export function createFlow(name?: string, description?: string, version?: string): FlowBuilder {
  return new FlowBuilder(name, description, version);
}

/**
 * Create a workflow input reference.
 */
export function input(path?: string, onSkip?: SkipAction): Value {
  return Value.input(path, onSkip);
}