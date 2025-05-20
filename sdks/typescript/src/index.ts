import { StepflowStdioServer } from './server';
import * as protocol from './protocol';
import * as transport from './transport';

export {
  StepflowStdioServer,
  protocol,
  transport
};

if (require.main === module) {
  const { main } = require('./main');
  main();
}