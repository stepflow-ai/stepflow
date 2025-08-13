#!/usr/bin/env node

import { program } from 'commander';
import { StepflowStdioServer } from './server';
import { StepflowHttpServer } from './http-server';
import { udf, udfSchema } from './udf';

interface CLIOptions {
  http: boolean;
  host: string;
  port: number;
}

export async function runCLI(): Promise<void> {
  program
    .name('stepflow-py')
    .description('Stepflow TypeScript SDK Server')
    .option('--http', 'Run in HTTP mode instead of stdio mode', false)
    .option('--host <host>', 'Host to bind to (HTTP mode only)', 'localhost')
    .option('--port <port>', 'Port to bind to (HTTP mode only)', '8080')
    .parse();

  const options = program.opts<CLIOptions>();

  // Create the stdio server (used by both modes)
  const server = new StepflowStdioServer();

  // Register built-in components
  registerBuiltinComponents(server);

  if (options.http) {
    // HTTP mode
    const httpServer = new StepflowHttpServer(
      server,
      options.host,
      parseInt(options.port.toString(), 10)
    );
    await httpServer.start();
  } else {
    // Stdio mode (default)
    server.run();
  }
}

function registerBuiltinComponents(server: StepflowStdioServer): void {
  // Register UDF component
  server.registerComponent(
    udf,
    'udf',
    udfSchema.component
  );

  // Register example components
  server.registerComponent(
    (input: { a: number; b: number }) => {
      return { result: input.a + input.b };
    },
    'add',
    {
      description: 'Adds two numbers together',
      inputSchema: {
        type: 'object',
        properties: {
          a: { type: 'number', description: 'First number' },
          b: { type: 'number', description: 'Second number' }
        },
        required: ['a', 'b']
      },
      outputSchema: {
        type: 'object',
        properties: {
          result: { type: 'number', description: 'Sum of a and b' }
        },
        required: ['result']
      }
    }
  );
}

if (require.main === module) {
  runCLI().catch(error => {
    console.error('Error:', error);
    process.exit(1);
  });
}