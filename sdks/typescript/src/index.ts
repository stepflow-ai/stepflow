import { StepflowStdioServer } from './server';
import { StepflowHttpServer } from './http-server';
import * as protocol from './protocol';
import * as transport from './transport';
import { udf, udfSchema } from './udf';
import { runCLI } from './cli';


export {
  StepflowStdioServer,
  StepflowHttpServer,
  protocol,
  transport,
  udf,
  udfSchema,
  runCLI
};

// Export commonly used types
export type { StepflowContext } from './transport';
export type { ComponentEntry } from './server';

if (require.main === module) {
  runCLI().catch(error => {
    console.error('Error:', error);
    process.exit(1);
  });
}