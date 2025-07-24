#!/usr/bin/env ts-node

/**
 * Comprehensive Flow Builder Example
 * 
 * This example demonstrates all features of the Flow Builder API:
 * - Basic workflow construction
 * - Input and output schemas
 * - Step references and input references
 * - Property access via proxy
 * - Error handling strategies
 * - Skip conditions
 * - Flow analysis and introspection
 */

import { createFlow, Value, input, OnError, StepReference, WorkflowInput } from '../src';

console.log('🏗️  Flow Builder Comprehensive Example\n');

// ===============================================
// 1. CREATE A COMPLEX WORKFLOW
// ===============================================

const flow = createFlow(
  'user-data-pipeline', 
  'A comprehensive pipeline for processing user data with validation, transformation, and storage',
  '1.0.0'
)
.setInputSchema({
  type: 'object',
  properties: {
    userId: { 
      type: 'string', 
      description: 'Unique user identifier' 
    },
    config: {
      type: 'object',
      properties: {
        database: { 
          type: 'string', 
          description: 'Database connection string' 
        },
        retries: { 
          type: 'number', 
          default: 3,
          description: 'Number of retry attempts' 
        },
        enableValidation: { 
          type: 'boolean', 
          default: true 
        },
        outputFormat: {
          type: 'string',
          enum: ['json', 'xml', 'yaml'],
          default: 'json'
        }
      },
      required: ['database']
    },
    metadata: {
      type: 'object',
      properties: {
        requestId: { type: 'string' },
        timestamp: { type: 'string' },
        source: { type: 'string' }
      }
    }
  },
  required: ['userId', 'config']
})
.setOutputSchema({
  type: 'object',
  properties: {
    user: {
      type: 'object',
      properties: {
        id: { type: 'string' },
        profile: { type: 'object' },
        preferences: { type: 'object' }
      }
    },
    processed: {
      type: 'object',
      properties: {
        data: { type: 'object' },
        transformations: { type: 'array' },
        validationResults: { type: 'object' }
      }
    },
    storage: {
      type: 'object',
      properties: {
        id: { type: 'string' },
        location: { type: 'string' },
        format: { type: 'string' }
      }
    },
    metadata: {
      type: 'object',
      properties: {
        processingTime: { type: 'number' },
        steps: { type: 'array' },
        status: { type: 'string' }
      }
    }
  }
});

// ===============================================
// 2. ADD STEPS WITH VARIOUS REFERENCE PATTERNS
// ===============================================

console.log('Adding workflow steps...\n');

// Step 1: Fetch user from database
const fetchUser = flow.addStep({
  id: 'fetch_user',
  component: '/api/users/get',
  input: {
    id: input('$.userId'),                          // Direct input reference
    connection: input('$.config.database'),         // Nested input reference
    requestId: input('$.metadata.requestId'),       // Optional field reference
    include: Value.literal(['profile', 'preferences', 'settings'])  // Literal array
  },
  inputSchema: {
    type: 'object',
    properties: {
      id: { type: 'string' },
      connection: { type: 'string' },
      requestId: { type: 'string' },
      include: { type: 'array', items: { type: 'string' } }
    }
  },
  onError: OnError.retry(3)  // Retry up to 3 times on failure
});

// Step 2: Validate user data (conditional execution)
const validateUser = flow.addStep({
  id: 'validate_user',
  component: '/validation/user',
  input: {
    user: fetchUser.ref,                            // Reference to entire step output
    validationRules: Value.literal({
      required: ['id', 'email', 'profile'],
      email: { format: 'email' },
      profile: { required: ['name'] }
    })
  },
  skipIf: Value.step('fetch_user', '$.error'),      // Skip if fetch_user has error
  onError: OnError.fail()  // Fail entire workflow if validation fails
});

// Step 3: Transform user data (using property access)
const transformUser = flow.addStep({
  id: 'transform_user',
  component: '/python/transform_user',
  input: {
    user: fetchUser.data,                           // Property access via proxy
    profile: (fetchUser as any).data.profile,      // Nested property access
    preferences: fetchUser.get('preferences'),      // Bracket notation access
    outputFormat: input('$.config.outputFormat'),   // Configuration reference
    transformations: Value.literal([
      { type: 'normalize', fields: ['email', 'phone'] },
      { type: 'enrich', source: 'external_api' },
      { type: 'format', style: 'camelCase' }
    ])
  },
  skipIf: (validateUser as any).shouldSkip,         // Skip if validation says so
  onError: OnError.default({                        // Use default on error
    user: fetchUser.ref,
    transformations: [],
    status: 'transformation_failed'
  })
});

// Step 4: Store processed data
const storeData = flow.addStep({
  id: 'store_data',
  component: '/storage/persist',
  input: {
    data: transformUser.ref,                        // Reference to transform output
    userId: input('$.userId'),                      // Original user ID
    connection: input('$.config.database'),         // Database connection
    metadata: {
      requestId: input('$.metadata.requestId'),
      timestamp: input('$.metadata.timestamp'),
      source: input('$.metadata.source'),
      processedBy: Value.literal('user-data-pipeline')
    }
  },
  onError: OnError.skip()  // Skip storage on error, don't fail workflow
});

// Step 5: Generate summary report
const generateReport = flow.addStep({
  id: 'generate_report',
  component: '/reporting/summary',
  input: {
    user: transformUser.get('user'),                // Specific field from transform
    storage: storeData.ref,                         // Storage results
    validation: (validateUser as any).results,      // Validation results
    config: {
      format: input('$.config.outputFormat'),
      includeMetadata: Value.literal(true),
      sections: Value.literal(['user', 'processing', 'storage'])
    }
  }
});

// ===============================================
// 3. SET WORKFLOW OUTPUT
// ===============================================

flow.setOutput({
  user: {
    id: input('$.userId'),
    profile: (fetchUser as any).data.profile,
    preferences: fetchUser.get('preferences')
  },
  processed: {
    data: transformUser.get('user'),
    transformations: (transformUser as any).applied_transformations,
    validationResults: validateUser.ref
  },
  storage: {
    id: (storeData as any).id,
    location: storeData.get('location'),
    format: input('$.config.outputFormat')
  },
  metadata: {
    processingTime: generateReport.get('processingTime'),
    steps: Value.literal(['fetch', 'validate', 'transform', 'store', 'report']),
    status: Value.literal('completed')
  }
});

// ===============================================
// 4. BUILD AND ANALYZE WORKFLOW
// ===============================================

console.log('Building workflow...\n');
const workflow = flow.build();

console.log('📊 Workflow Analysis:');
console.log(`Name: ${workflow.name}`);
console.log(`Description: ${workflow.description}`);
console.log(`Version: ${workflow.version}`);
console.log(`Steps: ${workflow.steps.length}`);

// Analyze references
const references = flow.getReferences();
const stepRefs = references.filter(ref => ref instanceof StepReference);
const inputRefs = references.filter(ref => ref instanceof WorkflowInput);

console.log(`\n🔗 References Found:`);
console.log(`Step references: ${stepRefs.length}`);
console.log(`Input references: ${inputRefs.length}`);
console.log(`Total references: ${references.length}`);

// Analyze individual steps
console.log(`\n📋 Step Analysis:`);
workflow.steps.forEach((step, index) => {
  const stepHandle = flow.step(step.id);
  const stepRefs = stepHandle?.getReferences() || [];
  console.log(`${index + 1}. ${step.id}: ${stepRefs.length} references`);
});

// ===============================================
// 5. DISPLAY GENERATED WORKFLOW STRUCTURE
// ===============================================

console.log('\n📄 Generated Workflow Structure:');
console.log('=====================================');

// Show a sample of the generated structure
const sampleStep = workflow.steps[2]; // Transform step
console.log(`\nSample Step (${sampleStep.id}):`);
console.log(JSON.stringify({
  id: sampleStep.id,
  component: sampleStep.component,
  input: sampleStep.input,
  skip_if: sampleStep.skip_if,
  on_error: sampleStep.on_error
}, null, 2));

console.log(`\nWorkflow Output Structure:`);
const outputSample = {
  user: (workflow.output as any).user,
  processed: (workflow.output as any).processed
};
console.log(JSON.stringify(outputSample, null, 2));

// ===============================================
// 6. DEMONSTRATE PROTOCOL FORMAT
// ===============================================

console.log('\n🔧 Protocol Format Examples:');
console.log('============================');

// Show how different values are serialized
const examples = {
  literalValue: Value.literal({ retries: 3 }).toValueTemplate(),
  stepReference: Value.step('fetch_user').toValueTemplate(),
  stepWithPath: Value.step('transform_user', '$.user.profile').toValueTemplate(),
  inputReference: Value.convertToValueTemplate(input('$.userId')),
  inputWithPath: Value.convertToValueTemplate(input('$.config.database'))
};

Object.entries(examples).forEach(([name, value]) => {
  console.log(`\n${name}:`);
  console.log(JSON.stringify(value, null, 2));
});

// ===============================================
// 7. SAVE WORKFLOW TO FILE
// ===============================================

import { writeFileSync } from 'fs';
import { join } from 'path';

const outputPath = join(__dirname, 'generated-workflow.json');
writeFileSync(outputPath, JSON.stringify(workflow, null, 2));

console.log(`\n💾 Workflow saved to: ${outputPath}`);

// ===============================================
// 8. DEMONSTRATE FLOW LOADING AND ANALYSIS
// ===============================================

console.log('\n🔄 Flow Loading and Analysis:');
console.log('=============================');

// Load workflow for analysis
import { FlowBuilder } from '../src';
const loadedBuilder = FlowBuilder.load(workflow);

console.log(`Loaded workflow: ${loadedBuilder.getName()}`);
console.log(`Step IDs: ${loadedBuilder.getStepIds().join(', ')}`);

// Analyze loaded workflow
const loadedReferences = loadedBuilder.getReferences();
console.log(`References in loaded workflow: ${loadedReferences.length}`);

// Show step details
const transformStep = loadedBuilder.step('transform_user');
if (transformStep) {
  const transformRefs = transformStep.getReferences();
  console.log(`Transform step has ${transformRefs.length} references`);
}

console.log('\n✅ Flow Builder example completed successfully!');
console.log('\nKey Features Demonstrated:');
console.log('- Fluent workflow construction API');
console.log('- Input/output schema definitions');
console.log('- Multiple reference patterns (input, step, property access)');
console.log('- Error handling strategies (retry, fail, skip, default)');
console.log('- Skip conditions for conditional execution');
console.log('- Protocol-compliant value serialization');
console.log('- Workflow analysis and introspection');
console.log('- Flow loading from existing definitions');