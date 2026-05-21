import { createContext, ReactNode, useContext } from "react";

// Shared context for the sync key so child panes can register with uPlot.sync()
const SyncKeyContext = createContext<string | null>(null);

export { SyncKeyContext };

export function useSyncKey(): string | null {
  return useContext(SyncKeyContext);
}

type Props = {
  syncKey?: string;
  children: ReactNode;
};

export function PaneStack({ syncKey, children }: Props) {
  return (
    <SyncKeyContext.Provider value={syncKey ?? null}>
      <div className="flex flex-col gap-2">{children}</div>
    </SyncKeyContext.Provider>
  );
}
