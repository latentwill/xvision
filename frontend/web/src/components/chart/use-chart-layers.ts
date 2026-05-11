// use-chart-layers.ts
import { useEffect, useState } from 'react';
import { DEFAULT_LAYERS, LayerKey, storageKey } from './chart-layers';

export function useChartLayers(surface: string) {
  const key = storageKey(surface);
  const [layers, setLayers] = useState<Record<LayerKey, boolean>>(() => {
    try {
      const raw = localStorage.getItem(key);
      if (raw) return { ...DEFAULT_LAYERS, ...JSON.parse(raw) };
    } catch {}
    return DEFAULT_LAYERS;
  });
  useEffect(() => { try { localStorage.setItem(key, JSON.stringify(layers)); } catch {} }, [layers, key]);
  function toggle(k: LayerKey) { setLayers((prev) => ({ ...prev, [k]: !prev[k] })); }
  function set<K extends LayerKey>(k: K, v: boolean) { setLayers((prev) => ({ ...prev, [k]: v })); }
  return { layers, toggle, set };
}
