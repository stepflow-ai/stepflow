import { StepflowStdioServer } from './server';
import * as protocol from './protocol';
import * as transport from './transport';

export {
  StepflowStdioServer,
  protocol,
  transport
};

// Export commonly used types
export type { StepflowContext } from './transport';
export type { ComponentEntry } from './server';

if (require.main === module) {
  const { main } = require('./main');
  main();
}