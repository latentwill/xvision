const TONE: Record<string, string> = {
  "Prompt tweak": "text-info border-info/40",
  "Threshold tune": "text-violet border-violet/40",
  "Agent +": "text-gold border-gold/40",
  "Agent −": "text-warn border-warn/40",
  "Model swap": "text-violet border-violet/40",
  "Regime detect swap": "text-info border-info/40",
  Experiment: "text-text-3 border-border",
};

export function ExperimentPill({ kind = "Experiment" }: { kind?: string }) {
  const tone = TONE[kind] ?? TONE.Experiment;
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded px-1.5 py-0.5 font-mono text-[10px] border ${tone}`}
    >
      <span className="h-1 w-1 rounded-full bg-current" aria-hidden />
      {kind}
    </span>
  );
}
