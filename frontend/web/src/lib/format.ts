// Number / date / currency formatters used across the SPA. Kept centralised so
// the prototype's tabular-nums conventions stay consistent.

const NUM = new Intl.NumberFormat("en-US");
const PCT = new Intl.NumberFormat("en-US", {
  style: "percent",
  maximumFractionDigits: 2,
});
const USD = new Intl.NumberFormat("en-US", {
  style: "currency",
  currency: "USD",
  maximumFractionDigits: 0,
});

export const fmt = {
  num: (n: number) => NUM.format(n),
  pct: (n: number) => PCT.format(n),
  usd: (n: number) => USD.format(n),
  signed: (n: number) =>
    `${n > 0 ? "+" : ""}${n.toFixed(2)}`,
};

export function formatCadence(minutes: number): string {
  if (!Number.isFinite(minutes) || minutes <= 0) {
    return "—";
  }

  if (minutes < 60) {
    return `${minutes}m`;
  }

  const hours = Math.floor(minutes / 60);
  const remainderMinutes = minutes % 60;

  if (remainderMinutes === 0) {
    return `${hours}h`;
  }

  return `${hours}h ${remainderMinutes}m`;
}
