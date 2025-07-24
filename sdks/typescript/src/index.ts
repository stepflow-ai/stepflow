import { StepflowStdioServer } from './server';
import { StepflowHttpServer } from './http-server';
import * as protocol from './protocol';
import * as transport from './transport';
import { udf, udfSchema } from './udf';
import { runCLI } from './cli';
import { FlowBuilder, createFlow, OnError, input } from './flow-builder';
import { Value, StepReference, WorkflowInput, JsonPath } from './value';

export {
  StepflowStdioServer,
  StepflowHttpServer,
  protocol,
  transport,
  udf,
  udfSchema,
  runCLI,
  FlowBuilder,
  createFlow,
  OnError,
  input,
  Value,
  StepReference,
  WorkflowInput,
  JsonPath
};

// Export commonly used types
export type { StepflowContext } from './transport';
export type { ComponentEntry } from './server';
export type { Flow, Step, Schema, OnErrorAction, StepHandle } from './flow-builder';
export type { Valuable, SkipAction, EscapedLiteral } from './value';
export type { ValueTemplate, ReferenceExpr, EscapedLiteralExpr } from './protocol';

if (require.main === module) {
  runCLI().catch(error => {
    console.error('Error:', error);
    process.exit(1);
  });
}