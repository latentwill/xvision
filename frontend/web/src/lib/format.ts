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
