// Visualizes every Chart2ThemeDefinition token across the three themes
// side-by-side so the design team can spot drift.

import { themeDefinitions, type Chart2ThemeDefinition, type ResolvedTheme } from "@/theme/themes";

const THEMES: ResolvedTheme[] = ["light", "dark"];

function Swatch({ color }: { color: string }) {
  return (
    <span
      className="inline-block w-5 h-5 rounded border border-border align-middle"
      style={{ backgroundColor: color }}
      title={color}
    />
  );
}

type SectionKey = keyof Chart2ThemeDefinition;
const SECTIONS: { key: SectionKey; label: string }[] = [
  { key: "surface", label: "Surface" },
  { key: "candle", label: "Candle" },
  { key: "overlay", label: "Overlays" },
  { key: "marker", label: "Markers" },
  { key: "position", label: "Position" },
  { key: "panes", label: "Panes" },
];

export function ChartLabTokens() {
  return (
    <div className="grid gap-6">
      {SECTIONS.map(({ key, label }) => {
        const sample = themeDefinitions[THEMES[0]].chart2[key] as Record<string, string>;
        const tokenKeys = Object.keys(sample);
        return (
          <section key={key}>
            <h3 className="text-[13px] font-medium text-text mb-2">{label}</h3>
            <table className="border-collapse text-[12px]">
              <thead>
                <tr>
                  <th className="text-left text-text-3 px-2 py-1">Token</th>
                  {THEMES.map((t) => (
                    <th key={t} className="text-left text-text-3 px-2 py-1">
                      {t}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {tokenKeys.map((tk) => (
                  <tr key={tk} className="border-t border-border-soft">
                    <td className="px-2 py-1 text-text-2">{tk}</td>
                    {THEMES.map((t) => {
                      const section = themeDefinitions[t].chart2[key] as Record<string, string>;
                      const v = section[tk];
                      return (
                        <td key={t} className="px-2 py-1">
                          <Swatch color={v} />{" "}
                          <span className="text-text-3 ml-1">{v}</span>
                        </td>
                      );
                    })}
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        );
      })}

      <section>
        <h3 className="text-[13px] font-medium text-text mb-2">Compare palette</h3>
        <div className="grid gap-2">
          {THEMES.map((t) => (
            <div key={t} className="flex items-center gap-2">
              <span className="text-text-3 w-24 text-[12px]">{t}</span>
              {themeDefinitions[t].chart2.compare.palette.map((c, i) => (
                <Swatch key={i} color={c} />
              ))}
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}
