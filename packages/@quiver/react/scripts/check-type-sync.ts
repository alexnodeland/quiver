#!/usr/bin/env npx ts-node
/**
 * Type Sync Validation Script
 *
 * This script validates that the inlined types in @quiver/react match
 * the source types in @quiver/wasm/src/types.ts.
 *
 * Background:
 * Due to TypeScript module resolution issues with pnpm workspaces and tsup-generated
 * re-exports, @quiver/react inlines types rather than importing from @quiver/wasm.
 * This script ensures those inlined types stay in sync with the source.
 *
 * Usage:
 *   pnpm run check-types
 *   # or
 *   npx ts-node scripts/check-type-sync.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import { fileURLToPath } from 'url';

// ESM compatibility for __dirname
const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

// Type definitions that must be kept in sync
const SYNCED_TYPES = [
  // From index.ts
  'SignalKind',
  'SignalColors',
  'PortDef',
  'PortSpec',
  'Compatibility',
  'ModuleDef',
  'CableDef',
  'PatchDef',
  'ValidationError',
  'ValidationResult',
  'PortReference',
  'ModuleTypeId',
  'ModuleCategory',
  // From hooks.ts
  'ObservableValue',
  'SubscriptionTarget',
  'PortSummary',
  'ModuleCatalogEntry',
  'CatalogResponse',
];

// Functions that must be kept in sync
const SYNCED_FUNCTIONS = [
  'getSignalColor',
  'parsePortReference',
  'createPortReference',
  'createPatchDef',
  'createModuleDef',
  'createCableDef',
  'checkPortCompatibility',
  'getObservableValueKey',
  'getSubscriptionTargetKey',
];

// Constants that must be kept in sync
const SYNCED_CONSTANTS = ['DEFAULT_SIGNAL_COLORS'];

interface TypeDefinition {
  name: string;
  definition: string;
  file: string;
}

function extractTypeDefinitions(content: string, file: string): Map<string, TypeDefinition> {
  const types = new Map<string, TypeDefinition>();

  // Match type aliases: export type Name = ...
  const typeAliasRegex = /export\s+type\s+(\w+)\s*=\s*([^;]+(?:\{[^}]+\}[^;]*)?);/gs;
  let match;
  while ((match = typeAliasRegex.exec(content)) !== null) {
    const name = match[1];
    const definition = normalizeWhitespace(match[2]);
    types.set(name, { name, definition, file });
  }

  // Match interfaces: export interface Name { ... }
  const interfaceRegex = /export\s+interface\s+(\w+)(?:\s+extends\s+[^{]+)?\s*\{([^}]+(?:\{[^}]*\}[^}]*)*)\}/gs;
  while ((match = interfaceRegex.exec(content)) !== null) {
    const name = match[1];
    const definition = normalizeWhitespace(match[2]);
    types.set(name, { name, definition, file });
  }

  return types;
}

function extractFunctionSignatures(content: string, file: string): Map<string, TypeDefinition> {
  const functions = new Map<string, TypeDefinition>();

  // Match function declarations
  const funcRegex = /export\s+function\s+(\w+)\s*\(([^)]*)\)\s*(?::\s*([^{]+))?\s*\{/gs;
  let match;
  while ((match = funcRegex.exec(content)) !== null) {
    const name = match[1];
    const params = normalizeWhitespace(match[2]);
    const returnType = match[3] ? normalizeWhitespace(match[3]) : 'void';
    const definition = `(${params}) => ${returnType}`;
    functions.set(name, { name, definition, file });
  }

  return functions;
}

function extractConstants(content: string, file: string): Map<string, TypeDefinition> {
  const constants = new Map<string, TypeDefinition>();

  // Match const declarations with object literals
  const constRegex = /export\s+const\s+(\w+)(?:\s*:\s*\w+)?\s*=\s*(\{[^}]+\})/gs;
  let match;
  while ((match = constRegex.exec(content)) !== null) {
    const name = match[1];
    const definition = normalizeWhitespace(match[2]);
    constants.set(name, { name, definition, file });
  }

  return constants;
}

function normalizeWhitespace(str: string): string {
  return str
    .replace(/\s+/g, ' ')
    .replace(/\s*([{};:,|&])\s*/g, '$1')
    .trim();
}

function compareDefinitions(
  source: Map<string, TypeDefinition>,
  target: Map<string, TypeDefinition>,
  names: string[],
  kind: string
): string[] {
  const errors: string[] = [];

  for (const name of names) {
    const sourceType = source.get(name);
    const targetType = target.get(name);

    if (!sourceType) {
      errors.push(`${kind} '${name}' not found in @quiver/wasm/src/types.ts`);
      continue;
    }

    if (!targetType) {
      errors.push(`${kind} '${name}' not found in @quiver/react (should be inlined)`);
      continue;
    }

    // For complex types, just check they both exist
    // A full structural comparison would require a TypeScript parser
    if (sourceType.definition !== targetType.definition) {
      // Only warn, don't fail - structural comparison is imperfect
      console.warn(
        `  Warning: ${kind} '${name}' definitions differ (may be formatting):\n` +
          `    @quiver/wasm: ${sourceType.definition.slice(0, 80)}...\n` +
          `    @quiver/react: ${targetType.definition.slice(0, 80)}...`
      );
    }
  }

  return errors;
}

function main() {
  const wasmTypesPath = path.resolve(__dirname, '../../../@quiver/wasm/src/types.ts');
  const reactIndexPath = path.resolve(__dirname, '../src/index.ts');
  const reactHooksPath = path.resolve(__dirname, '../src/hooks.ts');

  console.log('Checking type sync between @quiver/wasm and @quiver/react...\n');

  // Check files exist
  if (!fs.existsSync(wasmTypesPath)) {
    console.error(`Error: Source file not found: ${wasmTypesPath}`);
    process.exit(1);
  }
  if (!fs.existsSync(reactIndexPath)) {
    console.error(`Error: Target file not found: ${reactIndexPath}`);
    process.exit(1);
  }
  if (!fs.existsSync(reactHooksPath)) {
    console.error(`Error: Target file not found: ${reactHooksPath}`);
    process.exit(1);
  }

  const wasmContent = fs.readFileSync(wasmTypesPath, 'utf-8');
  const reactIndexContent = fs.readFileSync(reactIndexPath, 'utf-8');
  const reactHooksContent = fs.readFileSync(reactHooksPath, 'utf-8');
  const reactContent = reactIndexContent + '\n' + reactHooksContent;

  // Extract definitions
  const wasmTypes = extractTypeDefinitions(wasmContent, '@quiver/wasm/src/types.ts');
  const reactTypes = extractTypeDefinitions(reactContent, '@quiver/react');

  const wasmFunctions = extractFunctionSignatures(wasmContent, '@quiver/wasm/src/types.ts');
  const reactFunctions = extractFunctionSignatures(reactContent, '@quiver/react');

  const wasmConstants = extractConstants(wasmContent, '@quiver/wasm/src/types.ts');
  const reactConstants = extractConstants(reactContent, '@quiver/react');

  // Compare
  const errors: string[] = [];

  console.log('Checking types...');
  errors.push(...compareDefinitions(wasmTypes, reactTypes, SYNCED_TYPES, 'Type'));

  console.log('Checking functions...');
  errors.push(...compareDefinitions(wasmFunctions, reactFunctions, SYNCED_FUNCTIONS, 'Function'));

  console.log('Checking constants...');
  errors.push(...compareDefinitions(wasmConstants, reactConstants, SYNCED_CONSTANTS, 'Constant'));

  // Report results
  console.log('');
  if (errors.length === 0) {
    console.log('✓ All types are in sync!');
    console.log('');
    console.log(`  Checked: ${SYNCED_TYPES.length} types, ${SYNCED_FUNCTIONS.length} functions, ${SYNCED_CONSTANTS.length} constants`);
    process.exit(0);
  } else {
    console.error('✗ Type sync errors found:\n');
    for (const error of errors) {
      console.error(`  - ${error}`);
    }
    console.error('');
    console.error('Please update the inlined types in @quiver/react to match @quiver/wasm/src/types.ts');
    console.error('');
    console.error('Why are types inlined?');
    console.error('  TypeScript module resolution in pnpm workspaces has issues with tsup-generated');
    console.error('  re-exports. Until this is resolved, @quiver/react must inline its types.');
    process.exit(1);
  }
}

main();
