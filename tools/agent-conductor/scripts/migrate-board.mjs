#!/usr/bin/env node
// migrate-board.mjs — one-time migration from team/board.md (+ board-v2.md)
// into GitHub Issues + Project v2 items.
//
// Usage:
//   node migrate-board.mjs --board <path> [--board <path>] \
//                          --project <N> --repo <owner/name> \
//                          [--contracts-dir <path>] \
//                          [--schema <path>] \
//                          [--dry-run]
//
// Dry-run prints a JSON array of would-be task descriptors (each validates
// against board.schema.json) to stdout and makes no GraphQL/REST calls.
//
// Live mode:
//   1. For each parsed row, read the matching contract's front-matter to
//      enrich the descriptor (branch, worktree, depends_on, wave).
//   2. Validate the descriptor against the schema. Schema failure aborts
//      the run with a pointed error and a non-zero exit.
//   3. Find an existing Issue by the `track:<slug>` label. If absent,
//      create one. Add it as a Project item if not already attached.
//      Set each field that has changed. Existing Project state wins over
//      markdown on disagreement — print a warning to stderr and continue.
//
// Idempotent: re-running with no board changes produces no Issue creations
// and no field updates.
//
// The real `gh`-based client is in this file; tests pass a fake client via
// the exported `runMigration({ client, args })` entry point so unit tests
// stay network-free.

import { readFileSync, existsSync, readdirSync, statSync } from 'node:fs';
import { resolve, dirname, join, basename, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';

import { parseBoard } from './parse-board.mjs';

const execFileAsync = promisify(execFile);

// ---------- status mapping (contract front-matter → schema enum) ----------

const STATUS_MAP = Object.freeze({
  ready: 'READY',
  claimed: 'CLAIMED',
  'in-progress': 'CODING',
  'pr-open': 'PR_OPEN',
  'needs-rebase': 'CODING',
  merged: 'MERGED',
  archived: 'ARCHIVED',
  blocked: 'BACKLOG',
  deferred: 'BACKLOG',
  'scope-violation': 'BACKLOG',
});

function mapStatus(token) {
  if (!token) return 'BACKLOG';
  const mapped = STATUS_MAP[token.toLowerCase()];
  return mapped ?? 'BACKLOG';
}

// ---------- CLI parser ----------

export function parseArgs(argv) {
  const out = {
    boards: [],
    project: null,
    repo: null,
    contractsDir: null,
    schema: null,
    dryRun: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    switch (a) {
      case '--board':
        out.boards.push(argv[++i]);
        break;
      case '--project':
        out.project = Number(argv[++i]);
        break;
      case '--repo':
        out.repo = argv[++i];
        break;
      case '--contracts-dir':
        out.contractsDir = argv[++i];
        break;
      case '--schema':
        out.schema = argv[++i];
        break;
      case '--dry-run':
        out.dryRun = true;
        break;
      case '-h':
      case '--help':
        out.help = true;
        break;
      default:
        if (a.startsWith('--')) {
          throw new Error(`unknown flag: ${a}`);
        }
        throw new Error(`unexpected positional: ${a}`);
    }
  }
  return out;
}

// ---------- front-matter loader ----------
//
// Contracts use YAML front-matter, but the `acceptance:` and `verification:`
// blocks frequently contain free-form prose, backticked identifiers, and
// multi-line continuations that break strict YAML parsers (js-yaml etc.).
// migrate-board only needs a handful of top-level scalar fields, so we do
// a targeted extraction instead of full YAML parsing — robust against any
// content inside the block-mapped fields we don't read.

const FM_RE = /^---\s*\r?\n([\s\S]*?)\r?\n---\s*\r?\n/;

const SCALAR_FIELDS = Object.freeze([
  'track', 'lane', 'wave', 'worktree', 'branch', 'base', 'status',
]);

export function readContractFrontMatter(contractPath) {
  const raw = readFileSync(contractPath, 'utf8');
  const m = FM_RE.exec(raw);
  if (!m) {
    throw new Error(`no YAML front-matter in ${contractPath}`);
  }
  return extractScalarFields(m[1]);
}

/**
 * Extract the known scalar fields from front-matter text. Only top-level
 * lines matching `^<key>: <value>$` are considered; anything in a block
 * mapping below a key is ignored. Strips surrounding quotes from values.
 */
export function extractScalarFields(frontMatterText) {
  const out = {};
  const lines = frontMatterText.split(/\r?\n/);
  for (const line of lines) {
    // Only top-level (no leading whitespace) scalar assignments.
    const m = /^([a-z_][a-z0-9_]*):\s*(.*)$/i.exec(line);
    if (!m) continue;
    const key = m[1];
    if (!SCALAR_FIELDS.includes(key)) continue;
    let v = m[2].trim();
    if (v === '' || v === '[]' || v === '{}') continue;  // block-only / empty
    if ((v.startsWith('"') && v.endsWith('"')) ||
        (v.startsWith("'") && v.endsWith("'"))) {
      v = v.slice(1, -1);
    }
    // Strip inline trailing comments ("# comment").
    const hashIdx = v.indexOf(' #');
    if (hashIdx >= 0) v = v.slice(0, hashIdx).trim();
    out[key] = v;
  }
  return out;
}

// ---------- descriptor builder ----------

export function buildDescriptor({ row, frontMatter }) {
  const status = mapStatus(frontMatter.status ?? row.status);
  const lane = (frontMatter.lane ?? row.lane ?? '').toLowerCase();
  return {
    status,
    lane,
    track: frontMatter.track ?? row.track,
    owner_agent: null,
    branch: frontMatter.branch ?? null,
    worktree: frontMatter.worktree ?? null,
    pr: null,
    review_status: 'none',
    deploy_status: 'none',
    intake_doc: typeof frontMatter.wave === 'string'
      ? `team/intake/${frontMatter.wave}`
      : null,
  };
}

// ---------- schema validator ----------

export function compileValidator(schemaPath) {
  const ajv = new Ajv2020({ allErrors: true, strict: true });
  addFormats.default(ajv);
  const schema = JSON.parse(readFileSync(schemaPath, 'utf8'));
  return ajv.compile(schema);
}

function findDefaultSchemaPath(startDir) {
  let dir = startDir;
  for (let i = 0; i < 6; i++) {
    const candidate = join(dir, 'team', 'schema', 'board.schema.json');
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function findDefaultContractsDir(startDir) {
  let dir = startDir;
  for (let i = 0; i < 6; i++) {
    const candidate = join(dir, 'team', 'contracts');
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

// ---------- gh-backed client ----------

async function ghJson(args, { input } = {}) {
  const { stdout } = await execFileAsync('gh', args, {
    input,
    maxBuffer: 64 * 1024 * 1024,
  });
  return stdout.trim() === '' ? null : JSON.parse(stdout);
}

export function makeGhClient({ repo }) {
  const [owner, name] = String(repo).split('/');
  if (!owner || !name) throw new Error(`--repo must be <owner>/<name>, got: ${repo}`);

  return {
    async findIssueByTrackLabel(track) {
      const label = `track:${track}`;
      const data = await ghJson([
        'issue', 'list',
        '-R', `${owner}/${name}`,
        '--label', label,
        '--state', 'all',
        '--json', 'number,id,title,state',
      ]);
      const list = data ?? [];
      if (list.length === 0) return null;
      if (list.length > 1) {
        process.stderr.write(
          `migrate-board: WARNING multiple issues found with label ${label}; picking #${list[0].number}\n`,
        );
      }
      return { id: list[0].id, number: list[0].number };
    },
    async createIssue({ title, body, labels }) {
      const args = [
        'issue', 'create',
        '-R', `${owner}/${name}`,
        '--title', title,
        '--body', body,
      ];
      for (const l of labels ?? []) {
        args.push('--label', l);
      }
      const { stdout } = await execFileAsync('gh', args);
      const url = stdout.trim();
      const number = Number(url.split('/').pop());
      const issue = await ghJson([
        'issue', 'view', String(number),
        '-R', `${owner}/${name}`,
        '--json', 'id,number',
      ]);
      return { id: issue.id, number: issue.number };
    },
    async getProjectInfo({ projectOwner, number }) {
      const data = await ghJson([
        'project', 'view', String(number),
        '--owner', projectOwner,
        '--format', 'json',
      ]);
      const fields = await ghJson([
        'project', 'field-list', String(number),
        '--owner', projectOwner,
        '--format', 'json',
      ]);
      const byName = {};
      for (const f of fields?.fields ?? []) {
        byName[f.name] = {
          id: f.id,
          dataType: f.dataType,
          options: Object.fromEntries((f.options ?? []).map((o) => [o.name, o.id])),
        };
      }
      return { id: data.id, fields: byName };
    },
    async findProjectItem(projectId, contentId) {
      // The gh CLI does not expose Project items by content id directly; we
      // page through items and filter. For migration scale this is fine.
      const data = await ghJson([
        'project', 'item-list', '--owner', '@me',
        '--format', 'json',
        '--limit', '500',
        '--project-id', projectId,
      ]);
      const items = data?.items ?? [];
      const match = items.find((it) => it.content?.id === contentId);
      if (!match) return null;
      return { id: match.id, fieldValues: match.fieldValues ?? {} };
    },
    async addProjectItem(projectId, contentId) {
      const data = await ghJson([
        'project', 'item-add', '--owner', '@me',
        '--project-id', projectId,
        '--content-id', contentId,
        '--format', 'json',
      ]);
      return { id: data.id };
    },
    async setFieldValue({ projectId, itemId, fieldId, value, dataType }) {
      const args = [
        'project', 'item-edit', '--owner', '@me',
        '--project-id', projectId,
        '--id', itemId,
        '--field-id', fieldId,
      ];
      if (dataType === 'SINGLE_SELECT') {
        args.push('--single-select-option-id', value);
      } else if (dataType === 'NUMBER') {
        args.push('--number', String(value));
      } else if (dataType === 'DATE') {
        args.push('--date', String(value));
      } else {
        args.push('--text', String(value));
      }
      await execFileAsync('gh', args);
    },
  };
}

// ---------- migration driver (the testable bit) ----------

export async function runMigration({
  client,
  args,
  // injectable for tests
  readFile = (p) => readFileSync(p, 'utf8'),
  resolveContractPath,
  stdout = process.stdout,
  stderr = process.stderr,
}) {
  const opts = parseArgs(args);
  if (opts.help) {
    stdout.write(
      'migrate-board.mjs --board <path> [--board <path>] --project <N> --repo <owner/name>\n' +
      '                  [--contracts-dir <path>] [--schema <path>] [--dry-run]\n',
    );
    return { exitCode: 0 };
  }
  if (opts.boards.length === 0) {
    stderr.write('migrate-board: at least one --board <path> is required\n');
    return { exitCode: 2 };
  }
  if (!opts.dryRun && (!opts.project || !opts.repo)) {
    stderr.write('migrate-board: --project and --repo are required for live runs\n');
    return { exitCode: 2 };
  }

  const scriptDir = dirname(fileURLToPath(import.meta.url));
  const schemaPath = opts.schema ?? findDefaultSchemaPath(scriptDir);
  if (!schemaPath || !existsSync(schemaPath)) {
    stderr.write('migrate-board: could not locate team/schema/board.schema.json; pass --schema\n');
    return { exitCode: 2 };
  }
  const validate = compileValidator(schemaPath);
  const contractsDir = opts.contractsDir ?? findDefaultContractsDir(scriptDir);
  if (!contractsDir) {
    stderr.write('migrate-board: could not locate team/contracts/; pass --contracts-dir\n');
    return { exitCode: 2 };
  }
  const _resolveContractPath = resolveContractPath ?? ((row) => {
    const contractPath = isAbsolute(row.contractPath)
      ? row.contractPath
      : resolve(contractsDir, basename(row.contractPath));
    return contractPath;
  });

  // Parse all boards.
  const rows = [];
  for (const board of opts.boards) {
    const src = readFile(board);
    for (const row of parseBoard(src)) {
      rows.push({ ...row, sourceBoard: board });
    }
  }
  if (rows.length === 0) {
    stderr.write('migrate-board: no rows parsed from any board\n');
    return { exitCode: 1 };
  }

  // Build + validate descriptors.
  const plan = [];
  for (const row of rows) {
    const contractPath = _resolveContractPath(row);
    if (!existsSync(contractPath)) {
      stderr.write(`migrate-board: SKIP ${row.track} — contract not found at ${contractPath}\n`);
      continue;
    }
    let fm;
    try {
      const raw = readFile(contractPath);
      const m = FM_RE.exec(raw);
      if (!m) throw new Error('no front-matter');
      fm = extractScalarFields(m[1]);
    } catch (e) {
      stderr.write(`migrate-board: SKIP ${row.track} — bad contract front-matter: ${e.message}\n`);
      continue;
    }
    const descriptor = buildDescriptor({ row, frontMatter: fm });
    const ok = validate(descriptor);
    if (!ok) {
      stderr.write(
        `migrate-board: ABORT ${row.track} — descriptor fails schema:\n  ${(validate.errors ?? [])
          .map((e) => `${e.instancePath || '/'} ${e.message}`)
          .join('\n  ')}\n`,
      );
      return { exitCode: 1 };
    }
    plan.push({ row, descriptor, frontMatter: fm, contractPath });
  }

  if (opts.dryRun) {
    stdout.write(JSON.stringify(plan.map((p) => p.descriptor), null, 2) + '\n');
    return { exitCode: 0, plan };
  }

  if (!client) {
    stderr.write('migrate-board: live run requires a gh client\n');
    return { exitCode: 2 };
  }

  // Live: per-row reconcile.
  const projectOwner = opts.repo.split('/')[0];
  const project = await client.getProjectInfo({
    projectOwner,
    number: opts.project,
  });

  let created = 0;
  let updated = 0;
  let skipped = 0;

  for (const { row, descriptor, contractPath } of plan) {
    let issue = await client.findIssueByTrackLabel(descriptor.track);
    if (!issue) {
      issue = await client.createIssue({
        title: `[${descriptor.lane}] ${descriptor.track}`,
        body:
          `Contract: \`${contractPath.replace(/^.*team\//, 'team/')}\`\n\n` +
          `_Do not edit. Source of truth is the contract file._\n`,
        labels: [`track:${descriptor.track}`, `lane:${descriptor.lane}`],
      });
      created++;
    }

    let item = await client.findProjectItem(project.id, issue.id);
    if (!item) {
      const added = await client.addProjectItem(project.id, issue.id);
      item = { id: added.id, fieldValues: {} };
    }

    // Apply each field. Existing Project state wins on disagreement.
    const fieldUpdates = computeFieldUpdates({
      project,
      item,
      descriptor,
    });
    for (const update of fieldUpdates) {
      if (update.skipReason) {
        stderr.write(
          `migrate-board: WARN ${descriptor.track} field ${update.field} — ${update.skipReason}\n`,
        );
        skipped++;
        continue;
      }
      await client.setFieldValue({
        projectId: project.id,
        itemId: item.id,
        fieldId: update.fieldId,
        value: update.value,
        dataType: update.dataType,
      });
      updated++;
    }
  }

  stdout.write(`migrate-board: created=${created} field-updates=${updated} skipped=${skipped}\n`);
  return { exitCode: 0, created, updated, skipped };
}

export function computeFieldUpdates({ project, item, descriptor }) {
  const SINGLE = ['status', 'lane', 'review_status', 'deploy_status'];
  const TEXT = ['track', 'owner_agent', 'branch', 'worktree', 'intake_doc'];
  const NUMBER = ['pr'];

  const updates = [];

  for (const key of SINGLE) {
    const value = descriptor[key];
    if (value == null) continue;
    const field = project.fields[key];
    if (!field) {
      updates.push({ field: key, skipReason: 'field missing on Project — create it via the setup runbook' });
      continue;
    }
    const optionId = field.options[value];
    if (!optionId) {
      updates.push({ field: key, skipReason: `option ${value} missing on Project` });
      continue;
    }
    const existing = item.fieldValues?.[key];
    if (existing && existing.name && existing.name !== value) {
      updates.push({ field: key, skipReason: `Project has ${existing.name}, markdown wants ${value} — keeping Project state` });
      continue;
    }
    if (existing?.optionId === optionId || existing?.name === value) continue;
    updates.push({ field: key, fieldId: field.id, value: optionId, dataType: 'SINGLE_SELECT' });
  }

  for (const key of TEXT) {
    const value = descriptor[key];
    if (value == null || value === '') continue;
    const field = project.fields[key];
    if (!field) {
      updates.push({ field: key, skipReason: 'field missing on Project — create it via the setup runbook' });
      continue;
    }
    const existing = item.fieldValues?.[key];
    if (existing && existing.text && existing.text !== value) {
      updates.push({ field: key, skipReason: `Project has "${existing.text}", markdown wants "${value}" — keeping Project state` });
      continue;
    }
    if (existing?.text === value) continue;
    updates.push({ field: key, fieldId: field.id, value, dataType: 'TEXT' });
  }

  for (const key of NUMBER) {
    const value = descriptor[key];
    if (value == null) continue;
    const field = project.fields[key];
    if (!field) {
      updates.push({ field: key, skipReason: 'field missing on Project — create it via the setup runbook' });
      continue;
    }
    const existing = item.fieldValues?.[key];
    if (existing && existing.number != null && existing.number !== value) {
      updates.push({ field: key, skipReason: `Project has ${existing.number}, markdown wants ${value} — keeping Project state` });
      continue;
    }
    if (existing?.number === value) continue;
    updates.push({ field: key, fieldId: field.id, value, dataType: 'NUMBER' });
  }

  return updates;
}

// ---------- entry point ----------

const isMainModule = process.argv[1] && import.meta.url === `file://${process.argv[1]}`;
if (isMainModule) {
  const opts = parseArgs(process.argv.slice(2));
  const client = opts.dryRun ? null : makeGhClient({ repo: opts.repo });
  runMigration({ client, args: process.argv.slice(2) })
    .then((res) => process.exit(res.exitCode ?? 0))
    .catch((e) => {
      process.stderr.write(`migrate-board: ${e.message}\n`);
      process.exit(1);
    });
}
