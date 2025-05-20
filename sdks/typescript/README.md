# Stepflow TypeScript SDK

A TypeScript SDK for creating Stepflow component plugins. This SDK allows you to easily create and register components that can be used in Stepflow workflows.

## Installation

```bash
npm install stepflow-sdk
```

## Usage

### Creating a simple component

```typescript
import { StepflowStdioServer } from 'stepflow-sdk';

// Create server instance
const server = new StepflowStdioServer();

// Define input/output interfaces
interface AddInput {
  a: number;
  b: number;
}

interface AddOutput {
  result: number;
}

// Register a component using the decorator
class MathOperations {
  @server.component()
  static add(input: AddInput): AddOutput {
    return { result: input.a + input.b };
  }
}

// Set JSON schemas for the component
server.setComponentSchema(
  'add',
  {
    type: 'object',
    properties: {
      a: { type: 'number' },
      b: { type: 'number' }
    },
    required: ['a', 'b']
  },
  {
    type: 'object',
    properties: {
      result: { type: 'number' }
    },
    required: ['result']
  }
);

// Start the server
server.run();
```

### Custom component names

You can provide a custom name for your component:

```typescript
class TextOperations {
  @server.component({ name: 'concatenate' })
  static concat(input: { texts: string[] }): { result: string } {
    return { result: input.texts.join('') };
  }
}
```

## Development

### Building

```bash
npm run build
```

### Testing

```bash
npm test
```

### Linting

```bash
npm run lint
```

## License

MIT