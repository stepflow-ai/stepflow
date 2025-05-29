import { z } from 'zod';
import type { Schema, Step, StepExecution, Expr, JsonValue } from './types';

// JSON Schema type (simplified, can be extended as needed)
const jsonSchema = z.any();

// Helper type to convert Schema to ZodType
const schemaToZod = (schema: Schema): z.ZodType<any> => {
  // This is a simplified implementation
  // In a real-world scenario, you'd want to properly convert JSON Schema to Zod
  return z.any();
};

// Expression Schema - define the union type explicitly
const literalSchema = z.object({
  literal: z.any()
});

const inputRefSchema = z.object({
  input: z.string()
});

const stepRefSchema = z.object({
  step: z.string(),
  field: z.string().optional().nullable()
});

const exprSchema = z.union([
  literalSchema,
  inputRefSchema,
  stepRefSchema
]) as z.ZodType<Expr>;

// Step Execution Schema
const stepExecutionSchema = z.object({
  sideEffects: z.boolean().default(true),
  input_schema: jsonSchema.optional().nullable(),
  output_schema: jsonSchema.optional().nullable()
}) as z.ZodType<StepExecution>;

// Step Schema
const stepSchema = z.object({
  id: z.string(),
  component: z.string(),
  input: z.record(exprSchema),
  execution: stepExecutionSchema.optional().nullable()
}) as z.ZodType<Step>;

// Workflow Schema
export const workflowSchema = z.object({
  input_schema: jsonSchema.optional().nullable(),
  output_schema: jsonSchema.optional().nullable(),
  output: z.record(exprSchema).optional(),
  steps: z.array(stepSchema)
}) as z.ZodType<{
  input_schema?: Schema | null;
  output_schema?: Schema | null;
  outputs?: Record<string, Expr>;
  steps: Step[];
}>;

export type Workflow = z.infer<typeof workflowSchema>;
