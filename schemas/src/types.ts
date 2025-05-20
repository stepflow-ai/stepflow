// Base types
export type JsonValue = 
  | string 
  | number 
  | boolean 
  | null 
  | JsonValue[] 
  | { [key: string]: JsonValue };

export type Schema = JsonValue;

export interface StepExecution {
  sideEffects: boolean;
  input_schema?: Schema | null;
  output_schema?: Schema | null;
}

export interface Step {
  id: string;
  component: string;
  args: Record<string, Expr>;
  execution?: StepExecution | null;
}

export type Expr = 
  | { literal: JsonValue }
  | { input: string }
  | { step: string; field?: string | null };
