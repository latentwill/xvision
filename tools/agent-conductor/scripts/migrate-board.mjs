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

// ---------- gh argv builders (pure; unit-tested) ----------
//
// Building each gh command's argv as a pure function lets the tests assert
// the exact flags. This is how PR #290's review caught the regression
// where the live client used --project-id / --content-id on item-list and
// item-add — gh 2.65 rejects those. The argv builders below are the
// single source of truth; makeGhClient and the test fakes both consume
// them.

export function buildIssueListArgs({ repo, label }) {
  return [
    'issue', 'list',
    '-R', repo,
    '--label', label,
    '--state', 'all',
    '--json', 'number,id,title,state,url',
  ];
}

export function buildIssueCreateArgs({ repo, title, body, labels = [] }) {
  const args = ['issue', 'create', '-R', repo, '--title', title, '--body', body];
  for (const l of labels) args.push('--label', l);
  return args;
}

export function buildIssueViewArgs({ repo, number }) {
  return ['issue', 'view', String(number), '-R', repo, '--json', 'id,number,url'];
}

// Label pre-flight (Gap 2). `gh issue create --label <name>` aborts
// with "could not add label: '<name>' not found" if the label doesn't
// exist on the repo. The migration creates one issue per parsed track
// with `track:<slug>` + `lane:<lane>` labels, so we ensure the union
// set exists *before* any `gh issue create` runs. `--force` makes the
// create idempotent: existing labels keep their color/description.
export function buildLabelListArgs({ repo }) {
  return ['label', 'list', '-R', repo, '--limit', '200', '--json', 'name'];
}

export function buildLabelCreateArgs({ repo, name, color, description }) {
  return [
    'label', 'create', name,
    '-R', repo,
    '--color', color,
    '--description', description,
    '--force',
  ];
}

// Stable colour scheme for the migration's own labels. Tracks share a
// single colour because their identity comes from the slug, not the
// hue; lanes get distinct hues so the board is scannable at a glance.
export const MIGRATION_LABEL_COLORS = Object.freeze({
  'lane:foundation': { color: '0e8a16', description: 'Lane: foundation track' },
  'lane:integration': { color: '1d76db', description: 'Lane: integration track' },
  'lane:leaf': { color: 'fbca04', description: 'Lane: leaf track' },
  trackDefault: { color: '5319e7', description: 'Per-track tag (agent-conductor board)' },
});

export function requiredLabelsForDescriptors(descriptors) {
  const set = new Set();
  for (const d of descriptors) {
    if (d?.track) set.add(`track:${d.track}`);
    if (d?.lane) set.add(`lane:${d.lane}`);
  }
  return [...set].sort();
}

export function labelSpec(name) {
  if (name in MIGRATION_LABEL_COLORS) return MIGRATION_LABEL_COLORS[name];
  if (name.startsWith('track:')) return MIGRATION_LABEL_COLORS.trackDefault;
  // Fallback for an unknown lane name — surface as a neutral grey but
  // still create it so the issue-create doesn't blow up. The caller's
  // schema validation should catch malformed lanes upstream.
  return { color: 'ededed', description: `Label: ${name}` };
}

export function buildProjectViewArgs({ projectOwner, projectNumber }) {
  return ['project', 'view', String(projectNumber), '--owner', projectOwner, '--format', 'json'];
}

export function buildProjectFieldListArgs({ projectOwner, projectNumber }) {
  return ['project', 'field-list', String(projectNumber), '--owner', projectOwner, '--format', 'json', '--limit', '50'];
}

export function buildProjectItemListArgs({ projectOwner, projectNumber, limit = 500 }) {
  return [
    'project', 'item-list', String(projectNumber),
    '--owner', projectOwner,
    '--format', 'json',
    '--limit', String(limit),
  ];
}

export function buildProjectItemAddArgs({ projectOwner, projectNumber, issueUrl }) {
  return [
    'project', 'item-add', String(projectNumber),
    '--owner', projectOwner,
    '--url', issueUrl,
    '--format', 'json',
  ];
}

export function buildProjectItemEditArgs({
  projectId, itemId, fieldId, value, dataType,
}) {
  const args = [
    'project', 'item-edit',
    '--project-id', projectId,
    '--id', itemId,
    '--field-id', fieldId,
  ];
  switch (dataType) {
    case 'SINGLE_SELECT': args.push('--single-select-option-id', value); break;
    case 'NUMBER': args.push('--number', String(value)); break;
    case 'DATE': args.push('--date', String(value)); break;
    default: args.push('--text', String(value)); break;
  }
  return args;
}

// ---------- gh-backed client ----------

export function makeGhClient({ repo, projectOwner, projectNumber }) {
  const [owner, name] = String(repo).split('/');
  if (!owner || !name) throw new Error(`--repo must be <owner>/<name>, got: ${repo}`);
  if (!projectOwner || projectNumber == null) {
    throw new Error('makeGhClient: projectOwner and projectNumber are required');
  }
  const repoArg = `${owner}/${name}`;

  return {
    async ensureLabels(labels) {
      // Pre-flight: read once, create the missing ones with --force.
      // gh label create --force is idempotent (no-op on an exact match,
      // updates colour/description if either drifted). Done sequentially
      // to avoid hammering the API; the set is small (10-20 labels).
      const existing = new Set(
        ((await ghJson(buildLabelListArgs({ repo: repoArg }))) ?? []).map(
          (l) => l.name,
        ),
      );
      let created = 0;
      for (const name of labels) {
        if (existing.has(name)) continue;
        const spec = labelSpec(name);
        await execFileAsync(
          'gh',
          buildLabelCreateArgs({
            repo: repoArg,
            name,
            color: spec.color,
            description: spec.description,
          }),
        );
        created++;
      }
      return { ensured: labels.length, created };
    },
    async findIssueByTrackLabel(track) {
      const label = `track:${track}`;
      const data = await ghJson(buildIssueListArgs({ repo: repoArg, label }));
      const list = data ?? [];
      if (list.length === 0) return null;
      if (list.length > 1) {
        process.stderr.write(
          `migrate-board: WARNING multiple issues found with label ${label}; picking #${list[0].number}\n`,
        );
      }
      return { id: list[0].id, number: list[0].number, url: list[0].url };
    },
    async createIssue({ title, body, labels }) {
      const { stdout } = await execFileAsync(
        'gh',
        buildIssueCreateArgs({ repo: repoArg, title, body, labels }),
      );
      const url = stdout.trim();
      const number = Number(url.split('/').pop());
      const issue = await ghJson(buildIssueViewArgs({ repo: repoArg, number }));
      return { id: issue.id, number: issue.number, url: issue.url ?? url };
    },
    async getProjectInfo() {
      const data = await ghJson(buildProjectViewArgs({ projectOwner, projectNumber }));
      const fields = await ghJson(buildProjectFieldListArgs({ projectOwner, projectNumber }));
      // Keys are lower-cased on insert. GitHub Projects v2 ships every
      // new Project with a default `Status` field (capital S) that
      // cannot be deleted, renamed, or shadowed by a sibling `status`
      // field — so the schema-mandated `status` key has to match the
      // capital-S default in practice. Lower-casing here means
      // descriptor lookups (`project.fields['status']`) hit the right
      // field regardless of how GitHub capitalises the default.
      const byName = {};
      for (const f of fields?.fields ?? []) {
        byName[String(f.name).toLowerCase()] = {
          id: f.id,
          dataType: f.dataType,
          options: Object.fromEntries((f.options ?? []).map((o) => [o.name, o.id])),
        };
      }
      return { id: data.id, fields: byName };
    },
    async findProjectItem(_projectId, contentId) {
      // gh project item-list takes the project NUMBER + --owner, NOT
      // --project-id. The closure carries number+owner; the projectId
      // arg is accepted only for parity with the fake-client interface.
      const data = await ghJson(buildProjectItemListArgs({ projectOwner, projectNumber }));
      const items = data?.items ?? [];
      const match = items.find((it) => it.content?.id === contentId);
      if (!match) return null;
      return { id: match.id, fieldValues: match.fieldValues ?? {} };
    },
    async addProjectItem(_projectId, contentIdOrUrl, opts = {}) {
      // gh project item-add uses the issue URL, not the GraphQL content
      // id. Callers SHOULD pass the URL via opts.url; we fall back to
      // contentIdOrUrl if it looks like a URL.
      const url = opts.url
        ?? (typeof contentIdOrUrl === 'string' && contentIdOrUrl.startsWith('http')
              ? contentIdOrUrl
              : null);
      if (!url) {
        throw new Error('addProjectItem: issue URL required (gh project item-add --url)');
      }
      const data = await ghJson(buildProjectItemAddArgs({
        projectOwner, projectNumber, issueUrl: url,
      }));
      return { id: data.id };
    },
    async setFieldValue({ projectId, itemId, fieldId, value, dataType }) {
      await execFileAsync(
        'gh',
        buildProjectItemEditArgs({ projectId, itemId, fieldId, value, dataType }),
      );
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

  // Live: pre-flight the label set so `gh issue create --label` does
  // not abort on the first new track. `ensureLabels` is idempotent.
  if (typeof client.ensureLabels === 'function') {
    const labels = requiredLabelsForDescriptors(plan.map((p) => p.descriptor));
    await client.ensureLabels(labels);
  }

  // Per-row reconcile.
  const project = await client.getProjectInfo();

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
    if (!issue.url) {
      stderr.write(
        `migrate-board: ABORT ${descriptor.track} — Issue has no URL; ` +
        `cannot add to Project (gh project item-add requires --url).\n`,
      );
      return { exitCode: 1 };
    }

    let item = await client.findProjectItem(project.id, issue.id);
    if (!item) {
      const added = await client.addProjectItem(project.id, issue.id, { url: issue.url });
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
  const client = opts.dryRun
    ? null
    : makeGhClient({
        repo: opts.repo,
        projectOwner: opts.repo?.split('/')?.[0],
        projectNumber: opts.project,
      });
  runMigration({ client, args: process.argv.slice(2) })
    .then((res) => process.exit(res.exitCode ?? 0))
    .catch((e) => {
      process.stderr.write(`migrate-board: ${e.message}\n`);
      process.exit(1);
    });
}
