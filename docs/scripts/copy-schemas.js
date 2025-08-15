#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

// Define the source and destination paths
const sourceDir = path.join(__dirname, '../../schemas');
const destDir = path.join(__dirname, '../static/schemas');
const versionedDestDir = path.join(__dirname, '../static/schemas/v1');

// Schema files to copy (keeping original names)
const schemaFiles = [
  'protocol.json',
  'flow.json',
];

// Ensure destination directories exist
if (!fs.existsSync(destDir)) {
  fs.mkdirSync(destDir, { recursive: true });
}
if (!fs.existsSync(versionedDestDir)) {
  fs.mkdirSync(versionedDestDir, { recursive: true });
}

// Copy schemas
console.log('Copying schemas from source to docs...');

schemaFiles.forEach(schemaFile => {
  const sourcePath = path.join(sourceDir, schemaFile);
  const destPath = path.join(destDir, schemaFile);
  const versionedDestPath = path.join(versionedDestDir, schemaFile);

  if (fs.existsSync(sourcePath)) {
    // Copy to original location for backward compatibility
    fs.copyFileSync(sourcePath, destPath);
    console.log(`✓ Copied ${schemaFile} to schemas/`);
    
    // Copy to versioned location for new v1 paths
    fs.copyFileSync(sourcePath, versionedDestPath);
    console.log(`✓ Copied ${schemaFile} to schemas/v1/`);
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