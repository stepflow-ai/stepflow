#!/usr/bin/env ts-node

/**
 * HTTP Server Demo
 *
 * This example demonstrates how to run the Stepflow TypeScript SDK in HTTP mode.
 *
 * To run this example:
 * 1. Start the server: ts-node examples/http-server-demo.ts
 * 2. In another terminal, connect with SSE:
 *    curl -N http://localhost:8080/runtime/events
 * 3. Use the session ID from the endpoint event to make requests:
 *    curl -X POST http://localhost:8080/?sessionId=<SESSION_ID> \
 *      -H "Content-Type: application/json" \
 *      -d '{"jsonrpc":"2.0","id":1,"method":"components/list","params":{}}'
 */

import { runCLI } from '../src/cli';

// Override process.argv to enable HTTP mode
process.argv = ['node', 'http-server-demo.ts', '--http', '--port', '8080'];

// Run the CLI
runCLI().catch(error => {
  console.error('Error starting HTTP server:', error);
  process.exit(1);
});