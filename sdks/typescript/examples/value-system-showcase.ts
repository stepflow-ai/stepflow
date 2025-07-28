#!/usr/bin/env ts-node

/**
 * Value System Showcase
 * 
 * This example demonstrates all features of the Value system:
 * - Literal values with escaping
 * - Step references with JSON paths
 * - Workflow input references
 * - Property access via proxy
 * - Bracket notation access
 * - Complex nested structures
 * - Protocol format generation
 */

import { Value, input, StepReference, WorkflowInput, JsonPath } from '../src';

console.log('🔗 Value System Showcase\n');

// ===============================================
// 1. LITERAL VALUES
// ===============================================

console.log('1. 📝 Literal Values (Escaped in Protocol)');
console.log('==========================================');

const literals = {
  string: Value.literal('Hello, World!'),
  number: Value.literal(42),
  boolean: Value.literal(true),
  null: Value.literal(null),
  array: Value.literal([1, 2, 3, 'four', { five: 5 }]),
  object: Value.literal({
    name: 'Configuration',
    settings: {
      debug: true,
      level: 'info'
    },
    features: ['auth', 'logging', 'metrics']
  }),
  // Special case: literal that looks like a reference
  pseudoReference: Value.literal({
    $from: 'This is not a reference!',
    step: 'fake_step',
    path: '$.not.real'
  })
};

Object.entries(literals).forEach(([name, value]) => {
  console.log(`\n${name}:`);
  console.log('  Value:', JSON.stringify(value.getValue()));
  console.log('  Protocol:', JSON.stringify(value.toValueTemplate()));
});

// ===============================================
// 2. STEP REFERENCES
// ===============================================

console.log('\n\n2. 🎯 Step References');
console.log('====================');

const stepReferences = {
  // Basic step reference (entire output)
  basicStep: Value.step('user_processor'),
  
  // Step with JSON path
  withPath: Value.step('data_loader', '$.users[0].profile'),
  
  // Nested field access
  nestedField: Value.step('api_response', '$.data.items[0].metadata.created'),
  
  // Array access
  arrayAccess: Value.step('list_processor', '$.results[2]'),
  
  // Complex path
  complexPath: Value.step('nested_data', '$.config.database.connections[0].settings.timeout')
};

Object.entries(stepReferences).forEach(([name, value]) => {
  console.log(`\n${name}:`);
  console.log('  Protocol:', JSON.stringify(value.toValueTemplate(), null, 2));
});

// ===============================================
// 3. WORKFLOW INPUT REFERENCES
// ===============================================

console.log('\n\n3. 📥 Workflow Input References');
console.log('===============================');

const inputReferences = {
  // Root input
  root: input(),
  
  // Simple field
  userId: input('$.userId'),
  
  // Nested object access
  dbConfig: input('$.config.database.connection'),
  
  // Array element
  firstItem: input('$.items[0]'),
  
  // Complex nested path
  deepNested: input('$.user.profile.preferences.notifications[0].settings.enabled')
};

Object.entries(inputReferences).forEach(([name, value]) => {
  console.log(`\n${name}:`);
  console.log('  Protocol:', JSON.stringify(Value.convertToValueTemplate(value), null, 2));
});

// ===============================================
// 4. PROPERTY ACCESS VIA PROXY
// ===============================================

console.log('\n\n4. ⚡ Property Access via Proxy');
console.log('===============================');

// Create step references with proxy support
const userStep = StepReference.create('fetch_user');
const configInput = input();

console.log('Direct property access on step references:');

// These work via proxy magic - accessing properties that don't exist
// on the StepReference class creates new references with paths
const proxyExamples = {
  userName: (userStep as any).profile.name,
  userEmail: (userStep as any).contact.email,
  userPrefs: (userStep as any).settings.preferences,
  configDb: (configInput as any).database.connection,
  configTimeout: (configInput as any).api.timeout
};

Object.entries(proxyExamples).forEach(([name, ref]) => {
  console.log(`\n${name} -> ${ref.path?.toString() || 'N/A'}`);
  if (ref instanceof StepReference) {
    console.log('  Protocol:', JSON.stringify(ref.toReferenceExpr(), null, 2));
  } else if (ref instanceof WorkflowInput) {
    console.log('  Protocol:', JSON.stringify(ref.toReferenceExpr(), null, 2));
  }
});

// ===============================================
// 5. BRACKET NOTATION ACCESS
// ===============================================

console.log('\n\n5. 🔧 Bracket Notation Access');
console.log('=============================');

const stepRef = Value.step('data_processor');
const inputRef = input();

const bracketExamples = {
  // Numeric indices
  firstResult: stepRef.get(0),
  secondResult: stepRef.get(1),
  
  // String keys
  userField: stepRef.get('user'),
  dataField: stepRef.get('processed_data'),
  
  // Input bracket access
  configField: inputRef.get('config'),
  itemsArray: inputRef.get('items'),
  
  // Chained access
  chainedStep: stepRef.get('results').get(0).get('metadata'),
  chainedInput: inputRef.get('config').get('database').get('settings')
};

Object.entries(bracketExamples).forEach(([name, value]) => {
  console.log(`\n${name}:`);
  console.log('  Protocol:', JSON.stringify(Value.convertToValueTemplate(value), null, 2));
});

// ===============================================
// 6. COMPLEX NESTED STRUCTURES
// ===============================================

console.log('\n\n6. 🏗️  Complex Nested Structures');
console.log('=================================');

const complexStructure = {
  // Configuration section with mixed value types
  config: {
    database: {
      connection: input('$.config.database.url'),
      maxConnections: Value.literal(100),
      timeout: input('$.config.database.timeout'),
      retries: Value.literal(3)
    },
    features: {
      enabled: Value.literal(['auth', 'logging', 'metrics']),
      auth: {
        provider: input('$.config.auth.provider'),
        settings: Value.literal({
          tokenExpiry: '1h',
          refreshTokens: true,
          multiFactorAuth: false
        })
      }
    }
  },
  
  // Processing pipeline with step references
  pipeline: {
    input: input('$.payload'),
    steps: [
      Value.step('validator'),
      Value.step('transformer', '$.cleaned_data'),
      Value.step('enricher', '$.enhanced_data')
    ],
    output: Value.step('formatter', '$.final_result'),
    metadata: {
      processingTime: Value.step('metrics', '$.duration'),
      errors: Value.step('error_handler', '$.errors'),
      success: Value.literal(true)
    }
  },
  
  // User data with proxy-style access
  user: {
    id: input('$.userId'),
    profile: (Value.step('user_loader') as any).data.profile,
    preferences: Value.step('user_loader').get('preferences'),
    permissions: Value.literal(['read', 'write']),
    lastLogin: (input() as any).metadata.lastLogin
  },
  
  // Results aggregation
  results: {
    data: Value.step('aggregator', '$.results'),
    summary: {
      total: Value.step('counter', '$.total'),
      processed: Value.step('counter', '$.processed'),
      errors: Value.step('counter', '$.errors'),
      rate: Value.literal('N/A')  // Will be calculated later
    },
    exports: [
      {
        format: Value.literal('json'),
        location: Value.step('exporter', '$.json_location')
      },
      {
        format: Value.literal('csv'),
        location: Value.step('exporter', '$.csv_location')
      }
    ]
  }
};

console.log('Complex nested structure with protocol format:');
const protocolFormat = Value.convertToValueTemplate(complexStructure);
console.log(JSON.stringify(protocolFormat, null, 2));

// ===============================================
// 7. JSON PATH EXAMPLES
// ===============================================

console.log('\n\n7. 🛤️  JSON Path Examples');
console.log('=========================');

const pathExamples = [
  { description: 'Root access', path: '$' },
  { description: 'Simple field', path: '$.name' },
  { description: 'Nested field', path: '$.user.profile.name' },
  { description: 'Array element', path: '$.items[0]' },
  { description: 'Array field', path: '$.users[0].email' },
  { description: 'Deep nesting', path: '$.config.database.connections[0].settings.timeout' },
  { description: 'Multiple indices', path: '$.data[0].results[1].metadata' },
  { description: 'Mixed access', path: '$.system.logs[0].entries["error"].count' }
];

pathExamples.forEach(({ description, path }) => {
  const stepRef = Value.step('example_step', path);
  const inputRef = input(path);
  
  console.log(`\n${description}: ${path}`);
  console.log('  Step ref:', JSON.stringify(stepRef.toValueTemplate()));
  console.log('  Input ref:', JSON.stringify(Value.convertToValueTemplate(inputRef)));
});

// ===============================================
// 8. ERROR HANDLING WITH VALUES
// ===============================================

console.log('\n\n8. 🛡️  Error Handling with Values');
console.log('=================================');

const errorHandlingExamples = {
  // Skip actions for step references
  stepWithSkip: new StepReference('risky_step', new JsonPath(), {
    type: 'default',
    value: { error: 'Step failed', fallback: true }
  }),
  
  // Skip actions for input references  
  inputWithSkip: new WorkflowInput(new JsonPath(['$.optional_field']), {
    type: 'skip'
  }),
  
  // Default values
  stepWithDefault: new StepReference('optional_step', new JsonPath(), {
    type: 'default',
    value: null
  })
};

Object.entries(errorHandlingExamples).forEach(([name, ref]) => {
  console.log(`\n${name}:`);
  console.log('  Protocol:', JSON.stringify(ref.toReferenceExpr(), null, 2));
});

// ===============================================
// 9. PROTOCOL FORMAT COMPARISON
// ===============================================

console.log('\n\n9. 📋 Protocol Format Comparison');
console.log('================================');

const comparisonExamples = {
  literal: Value.literal('example'),
  stepBasic: Value.step('my_step'),
  stepWithPath: Value.step('my_step', '$.result.data'),
  inputBasic: input(),
  inputWithPath: input('$.config.setting'),
  mixed: {
    static: Value.literal('static_value'),
    dynamic: Value.step('processor', '$.output'),
    userInput: input('$.user.preference')
  }
};

console.log('Different value types and their protocol representations:\n');

Object.entries(comparisonExamples).forEach(([name, value]) => {
  console.log(`${name}:`);
  console.log('  Protocol:', JSON.stringify(Value.convertToValueTemplate(value), null, 2));
  console.log('');
});

console.log('\n✅ Value System showcase completed successfully!');
console.log('\nKey Features Demonstrated:');
console.log('- Literal value escaping with $literal');
console.log('- Step references with $from.step format');
console.log('- Input references with $from.workflow format');
console.log('- JSON path syntax for nested access');
console.log('- Property access via JavaScript proxy');
console.log('- Bracket notation for dynamic keys');
console.log('- Complex nested structure handling');
console.log('- Error handling with skip actions');
console.log('- Protocol-compliant serialization');