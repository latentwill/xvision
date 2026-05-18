// parse-board.mjs — extract task rows from team/board.md or team/board-v2.md.
//
// Exports a single function `parseBoard(markdownSource)` returning a typed
// array of rows. The board format is:
//
//   ### Section heading
//
//   - [<track>](contracts/<file>.md) - <lane> - <status> - <summary>.
//   - [<other-track>](contracts/<file>.md) - <lane> - <status> - <summary>.
//
// Rows occurring under sections that are NOT task lanes (e.g. "Deferred",
// "Recently Closed", "V2B+ Intake", "Reserved") are still parsed; the caller
// decides what to do based on the row's `section`. Non-row lines (headings,
// prose, blank lines) are skipped.
//
// The parser is intentionally forgiving: missing summary or lane tokens
// produce `null` instead of throwing. Schema-level validation is the caller's
// job — this function is a pure string→structure transform.

const ROW_RE = /^-\s+\[([^\]]+)\]\(([^)]+)\)\s*-\s*(.*)$/;
const SECTION_RE = /^###\s+(.+?)\s*$/;
const TOP_SECTION_RE = /^##\s+(.+?)\s*$/;

const KNOWN_LANES = new Set(['foundation', 'leaf', 'integration']);

/**
 * Parse a board markdown document into a list of rows.
 *
 * @param {string} markdownSource - Full board.md contents.
 * @returns {Array<{
 *   track: string,
 *   contractPath: string,
 *   lane: string | null,
 *   status: string | null,
 *   oneLineSummary: string,
 *   section: string | null,
 *   topSection: string | null,
 * }>}
 */
export function parseBoard(markdownSource) {
  const rows = [];
  let currentSection = null;
  let currentTopSection = null;

  const lines = String(markdownSource).split(/\r?\n/);
  for (const rawLine of lines) {
    const line = rawLine.replace(/\s+$/, '');

    const topMatch = TOP_SECTION_RE.exec(line);
    if (topMatch) {
      currentTopSection = topMatch[1];
      // A new top-level section clears the subsection context.
      currentSection = null;
      continue;
    }
    const subMatch = SECTION_RE.exec(line);
    if (subMatch) {
      currentSection = subMatch[1];
      continue;
    }
    const rowMatch = ROW_RE.exec(line);
    if (!rowMatch) continue;

    const track = rowMatch[1].trim();
    const contractPath = rowMatch[2].trim();
    const rest = rowMatch[3];

    // The rest is "lane - status - summary..." with the summary potentially
    // containing additional " - " separators. Split on the first two only.
    const parts = splitFirst(rest, ' - ', 2);
    const lane = (parts[0] || '').trim().toLowerCase() || null;
    const status = (parts[1] || '').trim().toLowerCase() || null;
    const oneLineSummary = (parts[2] || '').trim().replace(/\.$/, '');

    rows.push({
      track,
      contractPath,
      lane: KNOWN_LANES.has(lane) ? lane : null,
      status: status || null,
      oneLineSummary,
      section: currentSection,
      topSection: currentTopSection,
    });
  }

  return rows;
}

/**
 * Split a string on a separator at most `maxSplits` times. Returns up to
 * `maxSplits + 1` elements; the final element retains any remaining
 * occurrences of the separator.
 */
function splitFirst(input, sep, maxSplits) {
  const out = [];
  let remaining = input;
  for (let i = 0; i < maxSplits; i++) {
    const idx = remaining.indexOf(sep);
    if (idx < 0) break;
    out.push(remaining.slice(0, idx));
    remaining = remaining.slice(idx + sep.length);
  }
  out.push(remaining);
  return out;
}
