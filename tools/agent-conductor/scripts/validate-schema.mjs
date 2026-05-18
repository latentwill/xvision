#!/usr/bin/env node
// validate-schema.mjs — JSON Schema validator for the agent-cicd board.
//
// Usage:
//   node validate-schema.mjs <schema.json>
//       Validate <schema.json> itself as a JSON Schema 2020-12 document.
//
//   node validate-schema.mjs --check-examples <examples-dir>
//       Find ../../../team/schema/board.schema.json (or pass --schema <path>),
//       then validate every *.json under <examples-dir> against it.
//
// Exit non-zero on any failure. Stays vanilla Node + ajv; no build step.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve, join, basename, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';

const args = process.argv.slice(2);

function die(msg, code = 1) {
  process.stderr.write(`validate-schema: ${msg}\n`);
  process.exit(code);
}

function findFlag(name) {
  const i = args.indexOf(name);
  if (i < 0) return null;
  const v = args[i + 1];
  args.splice(i, 2);
  return v;
}

const checkExamples = args.includes('--check-examples');
if (checkExamples) args.splice(args.indexOf('--check-examples'), 1);

let schemaPath = findFlag('--schema');
const positional = args;

if (!schemaPath && !checkExamples && positional.length === 1) {
  schemaPath = positional[0];
} else if (checkExamples && !schemaPath) {
  const here = dirname(fileURLToPath(import.meta.url));
  schemaPath = resolve(here, '../../../team/schema/board.schema.json');
}

if (!schemaPath) die('missing schema path (pass as positional arg or --schema)');

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats.default(ajv);

let schema;
try {
  schema = JSON.parse(readFileSync(schemaPath, 'utf8'));
} catch (e) {
  die(`could not read schema ${schemaPath}: ${e.message}`);
}

let validate;
try {
  validate = ajv.compile(schema);
} catch (e) {
  die(`schema compile failed: ${e.message}`);
}

if (!checkExamples) {
  // Mode 1: validate the schema document itself by compile-success.
  process.stdout.write(`ok: ${basename(schemaPath)} compiles as JSON Schema 2020-12\n`);
  process.exit(0);
}

// Mode 2: validate every *.json in the positional dir against the schema.
const dir = positional[0];
if (!dir) die('missing examples directory');

let failed = 0;
let passed = 0;
const entries = readdirSync(dir).filter((f) => f.endsWith('.json')).sort();
if (entries.length === 0) die(`no *.json files found in ${dir}`);

for (const entry of entries) {
  const file = join(dir, entry);
  if (!statSync(file).isFile()) continue;
  let doc;
  try {
    doc = JSON.parse(readFileSync(file, 'utf8'));
  } catch (e) {
    process.stderr.write(`FAIL ${entry}: invalid JSON — ${e.message}\n`);
    failed++;
    continue;
  }
  const valid = validate(doc);
  if (valid) {
    process.stdout.write(`ok: ${entry}\n`);
    passed++;
  } else {
    process.stderr.write(`FAIL ${entry}:\n`);
    for (const err of validate.errors ?? []) {
      process.stderr.write(`  ${err.instancePath || '/'} ${err.message}\n`);
    }
    failed++;
  }
}

process.stdout.write(`\n${passed} passed, ${failed} failed\n`);
process.exit(failed === 0 ? 0 : 1);
