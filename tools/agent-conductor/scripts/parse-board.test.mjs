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

test('parseBoard handles the V2 board format identically', () => {
  const rows = parseBoard(SAMPLE_V2);
  assert.equal(rows.length, 1);
  assert.equal(rows[0].track, 'sample-v2-track');
  assert.equal(rows[0].section, 'Sample V2 Wave');
  assert.equal(rows[0].topSection, 'Active');
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
