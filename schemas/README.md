# StepFlow Schemas

TypeScript Zod schemas for the StepFlow project. This package provides type-safe validation for StepFlow's core data structures.

## Installation

```bash
npm install @stepflow/schemas zod
```

Note: `zod` is a peer dependency and must be installed alongside this package.

## Available Schemas

### Workflow Schema

Validates workflow definitions, including steps, inputs, outputs, and expressions.

```typescript
import { workflowSchema } from '@stepflow/schemas';

const result = workflowSchema.safeParse(workflowData);
if (result.success) {
  // Type-safe access to workflow data
  const workflow = result.data;
}
```

### Protocol Schema

Validates JSON-RPC 2.0 protocol messages used by StepFlow.

```typescript
import { protocolSchema } from '@stepflow/schemas';

const result = protocolSchema.safeParse(message);
if (result.success) {
  // Type-safe access to protocol message
  const message = result.data;
}
```

## Development

### Prerequisites

- Node.js 16+
- npm 7+

### Setup

1. Install dependencies:
   ```bash
   npm install
   ```

2. Build the project:
   ```bash
   npm run build
   ```

### Testing

Run the test suite:

```bash
npm test
```

Run tests in watch mode:

```bash
npm run test:watch
```

### Linting and Formatting

Run the linter:

```bash
npm run lint
```

Format code:

```bash
npm run format
```

## Versioning

This project follows [Semantic Versioning](https://semver.org/).

## License

MIT
