// CapabilityStatusBadge — a single typed capability-status chip.
//
// Maps each `CapabilityStatus` variant to a Pill tone so blockers read
// red/amber and the satisfied states read gold/info/muted. Dark-mode safe:
// the Pill primitive uses theme border + low-opacity colored tokens, never
// a white border.

import { Pill } from "@/components/primitives/Pill";
import {
  statusLabel,
  type CapabilityStatus,
} from "@/api/diagnostics";

type Tone = "default" | "gold" | "danger" | "warn" | "info";

function toneFor(status: CapabilityStatus): Tone {
  switch (status.kind) {
    case "ready":
      return "info";
    case "optimizable":
      return "gold";
    case "optional":
      return "default";
    case "missing_prompt":
    case "missing_model_binding":
    case "missing_tool":
      return "danger";
    case "unsupported":
      return "warn";
  }
}

export function CapabilityStatusBadge({
  status,
  className = "",
}: {
  status: CapabilityStatus;
  className?: string;
}) {
  const label =
    status.kind === "missing_tool"
      ? `Missing tool: ${status.tool}`
      : statusLabel(status);
  return (
    <Pill
      tone={toneFor(status)}
      className={className}
      data-testid={`cap-status-${status.kind}`}
    >
      {label}
    </Pill>
  );
}
