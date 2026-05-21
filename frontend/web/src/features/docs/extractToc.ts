// Parse an ordered list of H2/H3 headings out of a markdown body so the
// docs route can render a right-rail TOC + scrollspy. The slug logic
// mirrors `rehype-slug`'s github-slugger behavior so the in-app heading
// `id` attributes (injected by rehype-slug at render time) match the
// hrefs we generate here.

import GithubSlugger from "github-slugger";

export type TocItem = {
  id: string;
  text: string;
  level: 2 | 3;
};

const HEADING_RE = /^(#{2,3})\s+(.+?)\s*#*\s*$/;

export function extractToc(body: string): TocItem[] {
  const slugger = new GithubSlugger();
  const items: TocItem[] = [];
  let inFence = false;

  for (const raw of body.split(/\r?\n/)) {
    if (raw.startsWith("```") || raw.startsWith("~~~")) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;

    const m = HEADING_RE.exec(raw);
    if (!m) continue;
    const level = m[1].length === 2 ? 2 : 3;
    const text = m[2].replace(/[*_`]/g, "").trim();
    if (!text) continue;
    items.push({ id: slugger.slug(text), text, level: level as 2 | 3 });
  }

  return items;
}
