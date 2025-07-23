#!/usr/bin/env node

import { runCLI } from './cli';

// This is now just an entry point that delegates to the CLI
runCLI().catch(error => {
  console.error('Error:', error);
  process.exit(1);
});