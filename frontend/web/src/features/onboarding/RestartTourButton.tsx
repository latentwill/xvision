import { restartFirstRunTour } from "./useFirstRunTour";

export function RestartTourButton() {
  return (
    <button
      type="button"
      onClick={() => restartFirstRunTour()}
      className="rounded border border-border bg-surface-elev px-3 py-1.5 text-[12px] text-text-2 transition-colors hover:border-text-3 hover:text-text"
    >
      Restart tour
    </button>
  );
}
