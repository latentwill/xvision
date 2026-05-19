import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import { strict as assert } from 'node:assert';

import { parseBoard } from './parse-board.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const SAMPLE = readFileSync(resolve(here, 'fixtures/board-sample.md'), 'utf8');
const SAMPLE_V2 = readFileSync(resolve(here, 'fixtures/board-v2-sample.md'), 'utf8');

test('parseBoard extracts all rows from the sample board', () => {
  const rows = parseBoard(SAMPLE);
  const tracks = rows.map((r) => r.track);
  assert.deepEqual(tracks, [
    'sample-foundation-track',
    'sample-leaf-track',
    'sample-claimed-track',
    'sample-pr-open-track',
    'sample-deferred-track',
  ]);
});

test('parseBoard captures lane and status as lowercased tokens', () => {
  const rows = parseBoard(SAMPLE);
  const byTrack = Object.fromEntries(rows.map((r) => [r.track, r]));
  assert.equal(byTrack['sample-foundation-track'].lane, 'foundation');
  assert.equal(byTrack['sample-foundation-track'].status, 'ready');
  assert.equal(byTrack['sample-claimed-track'].lane, 'integration');
  assert.equal(byTrack['sample-claimed-track'].status, 'claimed');
  assert.equal(byTrack['sample-pr-open-track'].status, 'pr-open');
});

test('parseBoard preserves the section and top-level section context', () => {
  const rows = parseBoard(SAMPLE);
  const byTrack = Object.fromEntries(rows.map((r) => [r.track, r]));
  assert.equal(byTrack['sample-foundation-track'].section, 'Sample Wave A');
  assert.equal(byTrack['sample-foundation-track'].topSection, 'Active');
  assert.equal(byTrack['sample-claimed-track'].section, 'Sample Wave B');
  assert.equal(byTrack['sample-claimed-track'].topSection, 'Active');
  assert.equal(byTrack['sample-deferred-track'].topSection, 'Deferred');
});

test('parseBoard keeps the contract path as written', () => {
  const rows = parseBoard(SAMPLE);
  const byTrack = Object.fromEntries(rows.map((r) => [r.track, r]));
  assert.equal(
    byTrack['sample-leaf-track'].contractPath,
    'contracts/sample-leaf-track.md',
  );
});

test('parseBoard preserves extra " - " separators inside the summary', () => {
  const rows = parseBoard(SAMPLE);
  const pr = rows.find((r) => r.track === 'sample-pr-open-track');
  assert.ok(pr);
  assert.equal(pr.lane, 'leaf');
  assert.equal(pr.status, 'pr-open');
  // The fixture summary contains "Two extra - dashes - in the summary..."
  assert.ok(pr.oneLineSummary.includes('Two extra - dashes - in the summary'),
    `summary should retain inline dashes, got: ${pr.oneLineSummary}`);
});

test('parseBoard ignores prose lines that are not task rows', () => {
  const rows = parseBoard(SAMPLE);
  for (const r of rows) {
    assert.notEqual(r.track, '**Some prose entry**');
  }
});

test('parseBoard handles the V2 board format with em-dash separators', () => {
  // The V2 fixture uses "— · ·" instead of "- - -". Both must yield
  // the same structured output. Two rows: one leaf-ready, one
  // integration-pr-open.
  const rows = parseBoard(SAMPLE_V2);
  assert.equal(rows.length, 2);
  const tracks = rows.map((r) => r.track);
  assert.deepEqual(tracks, [
    'sample-v2-track',
    'sample-v2-integration-track',
  ]);
  for (const r of rows) {
    assert.equal(r.section, 'Sample V2 Wave');
    assert.equal(r.topSection, 'Active');
  }
  assert.equal(rows[0].lane, 'leaf');
  assert.equal(rows[0].status, 'ready');
  assert.equal(rows[1].lane, 'integration');
  assert.equal(rows[1].status, 'pr-open');
  // Summary normalisation: em-dash and middle-dot were rewritten to
  // ASCII hyphens before splitting, so the post-split summary is the
  // same string the V1 format would produce.
  assert.ok(rows[0].oneLineSummary.includes('sample V2-board row'));
});

test('parseBoard accepts mixed em-dash / hyphen rows in the same board', () => {
  // A single board file with both formats interleaved should not
  // confuse the parser. This guards against a regression where the
  // normaliser is accidentally applied per-section instead of
  // per-line.
  const md = `## Active
### X

- [hyphen-row](contracts/h.md) - leaf - ready - hyphen separators.
- [emdash-row](contracts/e.md) — leaf · ready · em-dash separators.
`;
  const rows = parseBoard(md);
  assert.equal(rows.length, 2);
  assert.deepEqual(rows.map((r) => r.track), ['hyphen-row', 'emdash-row']);
  for (const r of rows) {
    assert.equal(r.lane, 'leaf');
    assert.equal(r.status, 'ready');
  }
});

test('parseBoard leaves em-dash inside summary text intact', () => {
  // Only whitespace-bracketed em-dash / middle-dot are normalised, so
  // an em-dash inside a summary survives.
  const md = `## Active
### X

- [keep-emdash](contracts/k.md) - leaf - ready - summary with em—dash glued.
`;
  const rows = parseBoard(md);
  assert.equal(rows.length, 1);
  assert.ok(rows[0].oneLineSummary.includes('em—dash glued'),
    `summary should retain glued em-dash, got: ${rows[0].oneLineSummary}`);
});

test('parseBoard returns empty array for empty input', () => {
  assert.deepEqual(parseBoard(''), []);
  assert.deepEqual(parseBoard('   \n   \n'), []);
});

test('parseBoard sets lane to null for an unknown lane token', () => {
  const md = `## Active\n### X\n- [bad-lane](contracts/x.md) - banana - ready - whatever.\n`;
  const rows = parseBoard(md);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].lane, null);
  assert.equal(rows[0].status, 'ready');
});

test('parseBoard tolerates rows with no summary', () => {
  const md = `## Active\n### X\n- [no-summary](contracts/x.md) - leaf - ready\n`;
  const rows = parseBoard(md);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].track, 'no-summary');
  assert.equal(rows[0].lane, 'leaf');
  assert.equal(rows[0].status, 'ready');
  assert.equal(rows[0].oneLineSummary, '');
});
