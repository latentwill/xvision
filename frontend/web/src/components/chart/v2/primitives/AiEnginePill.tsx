/**
 * AiEnginePill — animated gold-dot pill used in the B3 AI annotation
 * dashboard header. The dot has a steady-state inner circle plus a
 * pulsing halo that expands + fades out (`aiPulse` keyframe, 1.8s
 * ease-out infinite). Both purely CSS — no JS animation loop.
 *
 * The keyframe is declared inline once at the head of the component
 * via a `<style>` tag so the primitive remains self-contained without
 * polluting globals.css.
 */
import type { ReactElement } from "react";

const KEYFRAME_ID = "xvn-ai-pulse-keyframe";

function ensureKeyframe(): void {
  if (typeof document === "undefined") return;
  if (document.getElementById(KEYFRAME_ID)) return;
  const tag = document.createElement("style");
  tag.id = KEYFRAME_ID;
  tag.textContent = `
@keyframes xvnAiPulse {
  0%   { transform: scale(1);   opacity: 0.7; }
  100% { transform: scale(3.4); opacity: 0; }
}
`;
  document.head.appendChild(tag);
}

export interface AiEnginePillProps {
  label?: string;
}

export function AiEnginePill({
  label = "AI Engine · live",
}: AiEnginePillProps): ReactElement {
  ensureKeyframe();
  return (
    <span
      className="inline-flex items-center gap-2 px-3 py-1 rounded-full text-[12px]"
      style={{
        backgroundColor: "rgba(0,230,118,0.10)",
        border: "1px solid rgba(0,230,118,0.45)",
        color: "var(--text)",
      }}
    >
      <span
        aria-hidden="true"
        style={{ position: "relative", display: "inline-block", width: 6, height: 6 }}
      >
        <span
          style={{
            position: "absolute",
            inset: 0,
            borderRadius: "50%",
            background: "var(--gold)",
            animation: "xvnAiPulse 1.8s ease-out infinite",
            transformOrigin: "center",
          }}
        />
        <span
          style={{
            position: "absolute",
            inset: 0,
            borderRadius: "50%",
            background: "var(--gold)",
          }}
        />
      </span>
      <span>{label}</span>
    </span>
  );
}
