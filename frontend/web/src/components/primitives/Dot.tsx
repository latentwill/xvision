type Tone = "gold" | "warn" | "danger" | "info" | "muted";

const TONE: Record<Tone, string> = {
  gold: "bg-gold",
  warn: "bg-warn",
  danger: "bg-danger",
  info: "bg-info",
  muted: "bg-text-3",
};

export function Dot({ tone = "muted", className = "" }: { tone?: Tone; className?: string }) {
  return (
    <span
      aria-hidden
      className={`inline-block w-[6px] h-[6px] rounded-full align-middle relative -top-px ${TONE[tone]} ${className}`}
    />
  );
}
