// Shared stub for Phase A routes. Real route bodies land in later plans:
//   /strategies → Plan 1 Phase B (this track, after engine-api)
//   /authoring  → Plan 3
//   /eval-runs  → Plan 2
//   /settings   → Plan 2 + Plan 6
//   /setup      → Plan 6 (Settings & Onboarding)

import type { ReactNode } from "react";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";

export function StubRoute({
  title,
  sub,
  body,
}: {
  title: string;
  sub?: string;
  body?: ReactNode;
}) {
  return (
    <>
      <Topbar title={title} sub={sub} />
      <Card className="px-6 py-12">
        <div className="text-center text-text-2">
          <div className="font-serif italic text-[28px] text-text-3 mb-3">
            coming soon
          </div>
          <p className="m-0 max-w-md mx-auto leading-snug">
            {body ?? (
              <>
                The <span className="text-text">{title.toLowerCase()}</span>{" "}
                surface is scaffolding only in Phase A. Real content lands in a
                follow-up plan.
              </>
            )}
          </p>
        </div>
      </Card>
    </>
  );
}
