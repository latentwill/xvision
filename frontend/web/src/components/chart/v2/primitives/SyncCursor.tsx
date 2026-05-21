/**
 * SyncCursor — marker component; renders nothing.
 *
 * The cursor sync is implemented by individual panes using uPlot.sync(syncKey)
 * and the kline→uplot bridge from adapters/sync-bridge.ts.
 * This component exists purely as a declarative signal in the JSX tree so that
 * the syncKey is explicit at the surface level and tooling can find all usages.
 */

type Props = {
  syncKey: string;
};

// eslint-disable-next-line @typescript-eslint/no-unused-vars
export function SyncCursor(_props: Props): null {
  return null;
}
