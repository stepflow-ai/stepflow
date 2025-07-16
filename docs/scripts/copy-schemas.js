#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

// Define the source and destination paths
const sourceDir = path.join(__dirname, '../../schemas');
const destDir = path.join(__dirname, '../static/schemas');

// Schema files to copy (keeping original names)
const schemaFiles = [
  'protocol.json'
];

// Ensure destination directory exists
if (!fs.existsSync(destDir)) {
  fs.mkdirSync(destDir, { recursive: true });
}

// Copy schemas
console.log('Copying schemas from source to docs...');

schemaFiles.forEach(schemaFile => {
  const sourcePath = path.join(sourceDir, schemaFile);
  const destPath = path.join(destDir, schemaFile);
  
  if (fs.existsSync(sourcePath)) {
    fs.copyFileSync(sourcePath, destPath);
    console.log(`✓ Copied ${schemaFile}`);
  } else {
    console.warn(`⚠ Warning: Source file ${sourcePath} not found`);
  }
});

// Keep jsonSchema.json as it's needed for resolving JSON Schema references
const jsonSchemaPath = path.join(destDir, 'jsonSchema.json');
if (fs.existsSync(jsonSchemaPath)) {
  console.log('✓ Keeping existing jsonSchema.json');
} else {
  console.warn('⚠ Warning: jsonSchema.json not found - this may be needed for schema references');
}

console.log('Schema copying complete!');