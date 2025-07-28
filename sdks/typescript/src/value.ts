/**
 * Value API for creating workflow values and references.
 * This provides a TypeScript implementation of the Python SDK's Value system.
 */

import { ValueTemplate, ReferenceExpr, EscapedLiteralExpr, SkipAction } from './protocol';

export type Valuable = 
  | string
  | number
  | boolean
  | null
  | undefined
  | Record<string, any>
  | Array<any>
  | StepReference
  | WorkflowInput
  | EscapedLiteral
  | Value;

export interface EscapedLiteral {
  field_literal: any;
}

// Re-export protocol types for convenience
export { SkipAction } from './protocol';

/**
 * Base class for handling JSON Path syntax across all reference types.
 */
export class JsonPath {
  private fragments: string[] = ['$'];

  constructor(fragments?: string[]) {
    if (fragments) {
      this.fragments = [...fragments];
    }
  }

  /**
   * Add a field access to the path (e.g., .fieldName). Mutates this instance.
   */
  pushField(name: string): JsonPath {
    this.fragments.push(`.${name}`);
    return this;
  }

  /**
   * Add an index access to the path (e.g., [0] or ["key"]). Mutates this instance.
   */
  pushIndex(key: string | number): JsonPath {
    if (typeof key === 'number') {
      this.fragments.push(`[${key}]`);
    } else {
      this.fragments.push(`["${key}"]`);
    }
    return this;
  }

  /**
   * Create a new path with a field access.
   */
  withField(name: string): JsonPath {
    return this.copy().pushField(name);
  }

  /**
   * Create a new path with an index access.
   */
  withIndex(key: string | number): JsonPath {
    return this.copy().pushIndex(key);
  }

  /**
   * Create a copy of this JsonPath instance.
   */
  copy(): JsonPath {
    return new JsonPath([...this.fragments]);
  }

  /**
   * Return the path string, defaulting to '$' for root.
   */
  toString(): string {
    return this.fragments.join('');
  }

  /**
   * Get the fragments array.
   */
  getFragments(): string[] {
    return [...this.fragments];
  }

  /**
   * Set the entire path from a string.
   */
  setPath(path: string): JsonPath {
    this.fragments = [path];
    return this;
  }
}

/**
 * A reference to a step's output or a field within it.
 */
export class StepReference {
  constructor(
    public readonly stepId: string,
    public readonly path: JsonPath = new JsonPath(),
    public readonly onSkip?: SkipAction
  ) {}

  /**
   * Create a nested reference using index access.
   */
  [Symbol.toPrimitive](hint: string): any {
    if (hint === 'string') {
      return `StepReference(${this.stepId}, ${this.path.toString()})`;
    }
    return this;
  }

  /**
   * Create a nested reference using bracket notation.
   */
  get(key: string | number): StepReference {
    const newPath = this.path.withIndex(key);
    return new StepReference(this.stepId, newPath, this.onSkip);
  }

  /**
   * Create a nested reference to a field using a proxy for property access.
   */
  private static createProxy(ref: StepReference): StepReference {
    return new Proxy(ref, {
      get(target, prop) {
        // Preserve existing methods
        if (prop in target || typeof prop === 'symbol') {
          return (target as any)[prop];
        }
        
        if (typeof prop === 'string' && !prop.startsWith('_')) {
          const newPath = target.path.withField(prop);
          return StepReference.createProxy(new StepReference(target.stepId, newPath, target.onSkip));
        }
        return (target as any)[prop];
      }
    });
  }

  /**
   * Create a new StepReference with proxy support for field access.
   */
  static create(stepId: string, path?: JsonPath, onSkip?: SkipAction): StepReference {
    const ref = new StepReference(stepId, path, onSkip);
    return StepReference.createProxy(ref);
  }

  /**
   * Create a copy of this reference with the specified onSkip action.
   */
  withOnSkip(onSkip: SkipAction): StepReference {
    return new StepReference(this.stepId, this.path, onSkip);
  }

  /**
   * Convert to ReferenceExpr object for protocol.
   */
  toReferenceExpr(): ReferenceExpr {
    const expr: ReferenceExpr = {
      $from: { step: this.stepId }
    };
    
    const pathStr = this.path.toString();
    if (pathStr !== '$') {
      expr.path = pathStr;
    }
    
    if (this.onSkip) {
      expr.onSkip = this.onSkip;
    }
    
    return expr;
  }

  /**
   * @deprecated Use toReferenceExpr() instead
   */
  toReference(): any {
    return {
      step: {
        step_id: this.stepId,
        path: this.path.toString(),
        on_skip: this.onSkip
      }
    };
  }
}

/**
 * Reference to workflow input.
 */
export class WorkflowInput {
  constructor(
    public readonly path: JsonPath = new JsonPath(),
    public readonly onSkip?: SkipAction
  ) {}

  /**
   * Create a reference to a specific path in the workflow input using bracket notation.
   */
  get(key: string | number): WorkflowInput {
    const newPath = this.path.withIndex(key);
    return new WorkflowInput(newPath, this.onSkip);
  }

  /**
   * Create a nested reference to a field using a proxy for property access.
   */
  private static createProxy(input: WorkflowInput): WorkflowInput {
    return new Proxy(input, {
      get(target, prop) {
        // Preserve existing methods
        if (prop in target || typeof prop === 'symbol') {
          return (target as any)[prop];
        }
        
        if (typeof prop === 'string' && !prop.startsWith('_')) {
          const newPath = target.path.withField(prop);
          return WorkflowInput.createProxy(new WorkflowInput(newPath, target.onSkip));
        }
        return (target as any)[prop];
      }
    });
  }

  /**
   * Create a new WorkflowInput with proxy support for field access.
   */
  static create(path?: JsonPath, onSkip?: SkipAction): WorkflowInput {
    const input = new WorkflowInput(path, onSkip);
    return WorkflowInput.createProxy(input);
  }

  /**
   * Create a copy of this reference with the specified onSkip action.
   */
  withOnSkip(onSkip: SkipAction): WorkflowInput {
    return new WorkflowInput(this.path, onSkip);
  }

  /**
   * Convert to ReferenceExpr object for protocol.
   */
  toReferenceExpr(): ReferenceExpr {
    const expr: ReferenceExpr = {
      $from: { workflow: "input" }
    };
    
    const pathStr = this.path.toString();
    if (pathStr !== '$') {
      expr.path = pathStr;
    }
    
    if (this.onSkip) {
      expr.onSkip = this.onSkip;
    }
    
    return expr;
  }

  /**
   * @deprecated Use toReferenceExpr() instead
   */
  toReference(): any {
    return {
      input: {
        path: this.path.toString(),
        on_skip: this.onSkip
      }
    };
  }
}

/**
 * A value that can be used in workflow definitions.
 * 
 * This class provides a unified interface for creating values that can be:
 * - Literal values (using Value.literal() or new Value())
 * - References to steps (using Value.step())
 * - References to workflow input (using Value.input())
 */
export class Value {
  private _value: any;

  constructor(value: Valuable) {
    this._value = this.unwrapValue(value);
  }

  private unwrapValue(value: Valuable): any {
    if (value instanceof Value) {
      return value._value;
    }
    return value;
  }

  /**
   * Create a literal value that won't be expanded as a reference.
   * This is equivalent to using $literal in the workflow definition.
   */
  static literal(value: any): Value {
    return new Value({ field_literal: value } as EscapedLiteral);
  }

  /**
   * Create a reference to a step's output.
   * This is equivalent to using $from with a step reference.
   */
  static step(stepId: string, path?: string, onSkip?: SkipAction): Value {
    const jsonPath = new JsonPath();
    if (path && path !== '$') {
      jsonPath.setPath(path);
    }
    return new Value(new StepReference(stepId, jsonPath, onSkip));
  }

  /**
   * Create a reference to workflow input.
   * This is equivalent to using $from with a workflow input reference.
   */
  static input(path?: string, onSkip?: SkipAction): Value {
    const jsonPath = new JsonPath();
    if (path && path !== '$') {
      jsonPath.setPath(path);
    }
    return new Value(WorkflowInput.create(jsonPath, onSkip));
  }

  /**
   * Convert this Value to a ValueTemplate for use in flow definitions.
   */
  toValueTemplate(): ValueTemplate {
    return Value.convertToValueTemplate(this._value);
  }

  /**
   * Convert arbitrary data to ValueTemplate.
   */
  static convertToValueTemplate(data: any): ValueTemplate {
    if (data === null || data === undefined) {
      return data;
    }

    if (data instanceof Value) {
      return Value.convertToValueTemplate(data._value);
    }

    if (data instanceof StepReference) {
      return data.toReferenceExpr();
    }

    if (data instanceof WorkflowInput) {
      return data.toReferenceExpr();
    }

    if (data && typeof data === 'object' && 'field_literal' in data) {
      return { $literal: (data as EscapedLiteral).field_literal };
    }

    if (Array.isArray(data)) {
      return data.map(item => Value.convertToValueTemplate(item));
    }

    if (data && typeof data === 'object') {
      const result: Record<string, any> = {};
      for (const [key, value] of Object.entries(data)) {
        result[key] = Value.convertToValueTemplate(value);
      }
      return result;
    }

    return data;
  }

  /**
   * Get the underlying value.
   */
  getValue(): any {
    return this._value;
  }

  /**
   * Create a nested reference using bracket notation (when Value wraps a reference).
   */
  get(key: string | number): Value {
    if (this._value instanceof StepReference) {
      return new Value(this._value.get(key));
    }
    if (this._value instanceof WorkflowInput) {
      return new Value(this._value.get(key));
    }
    throw new Error('get() can only be called on Value instances that wrap StepReference or WorkflowInput');
  }
}