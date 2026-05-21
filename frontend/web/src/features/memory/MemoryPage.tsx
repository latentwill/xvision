// MemoryPage — workspace-level memory surface mounted at `/memory`.
//
// Scope: `namespace = "global"`. This is the operator's primary entry
// point for seeding cross-agent operator wisdom (Patterns) and
// reviewing autoresearcher-emitted global Observations.
//
// Sibling-page to `/agents/<id>` Memory tab: the per-agent tab is the
// only place to manage `agent:<id>` namespaces; this page is the only
// place to manage `global`. The MemorySurface component encodes that
// asymmetry so neither page leaks the other's affordances.
//
// `?pattern=<id>` query param: when arriving from the eval-review
// MemoryPanel's "Open Pattern" deep-link, the matching row highlights
// and scrolls into view.

import { useSearchParams } from "react-router-dom";

import { Topbar } from "@/components/shell/Topbar";
import { MemorySurface } from "./MemorySurface";

export function MemoryPage() {
  const [params] = useSearchParams();
  const highlight = params.get("pattern");

  return (
    <>
      <Topbar
        title="Memory"
        sub="Global namespace · Operator patterns and autoresearcher observations"
      />
      <MemorySurface mode="workspace" highlightPatternId={highlight} />
    </>
  );
}
