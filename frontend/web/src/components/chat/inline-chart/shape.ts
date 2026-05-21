import type { ChartBounds, InlineChartSeries, InlinePoint, NormalizedPoint, ViewBox } from "./types";

export const DEFAULT_VIEWBOX: ViewBox = {
  width: 300,
  height: 112,
  padX: 8,
  padY: 10,
};

export function seriesBounds(series: InlineChartSeries[]): ChartBounds {
  const points = series.flatMap((s) => s.points);
  if (points.length === 0) {
    return { minX: 0, maxX: 1, minY: 0, maxY: 1 };
  }

  let minX = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const point of points) {
    if (!Number.isFinite(point.x) || !Number.isFinite(point.y)) continue;
    minX = Math.min(minX, point.x);
    maxX = Math.max(maxX, point.x);
    minY = Math.min(minY, point.y);
    maxY = Math.max(maxY, point.y);
  }

  if (!Number.isFinite(minX) || !Number.isFinite(minY)) {
    return { minX: 0, maxX: 1, minY: 0, maxY: 1 };
  }
  if (minX === maxX) maxX = minX + 1;
  if (minY === maxY) {
    minY -= 1;
    maxY += 1;
  }
  return { minX, maxX, minY, maxY };
}

export function normalizePoints(
  points: InlinePoint[],
  bounds: ChartBounds,
  viewBox: ViewBox = DEFAULT_VIEWBOX,
): NormalizedPoint[] {
  const innerW = viewBox.width - viewBox.padX * 2;
  const innerH = viewBox.height - viewBox.padY * 2;
  const xSpan = bounds.maxX - bounds.minX || 1;
  const ySpan = bounds.maxY - bounds.minY || 1;

  return points
    .filter((point) => Number.isFinite(point.x) && Number.isFinite(point.y))
    .map((point) => ({
      ...point,
      sx: viewBox.padX + ((point.x - bounds.minX) / xSpan) * innerW,
      sy: viewBox.padY + (1 - (point.y - bounds.minY) / ySpan) * innerH,
    }));
}

export function linePath(points: NormalizedPoint[]): string {
  if (points.length === 0) return "";
  if (points.length === 1) {
    const p = points[0];
    return `M ${p.sx - 2} ${p.sy} L ${p.sx + 2} ${p.sy}`;
  }
  return points
    .map((point, index) => `${index === 0 ? "M" : "L"} ${point.sx} ${point.sy}`)
    .join(" ");
}

export function areaPath(
  points: NormalizedPoint[],
  viewBox: ViewBox = DEFAULT_VIEWBOX,
): string {
  if (points.length === 0) return "";
  const first = points[0];
  const last = points[points.length - 1];
  const baseline = viewBox.height - viewBox.padY;
  return `${linePath(points)} L ${last.sx} ${baseline} L ${first.sx} ${baseline} Z`;
}

export function histogramBars(
  points: InlinePoint[],
  viewBox: ViewBox = DEFAULT_VIEWBOX,
) {
  const finitePoints = points.filter(
    (point) => Number.isFinite(point.x) && Number.isFinite(point.y),
  );
  if (finitePoints.length === 0) return [];
  const maxAbs = Math.max(...finitePoints.map((point) => Math.abs(point.y)), 1);
  const innerW = viewBox.width - viewBox.padX * 2;
  const slotW = innerW / finitePoints.length;
  const gap = finitePoints.length > 1 ? Math.min(2, slotW * 0.25) : 0;
  const barW = Math.max(0, slotW - gap);
  const zeroY = viewBox.height / 2;
  const maxH = viewBox.height / 2 - viewBox.padY;

  return finitePoints.map((point, index) => {
    const h = Math.max(1, (Math.abs(point.y) / maxAbs) * maxH);
    const positive = point.y >= 0;
    return {
      x: viewBox.padX + index * slotW,
      y: positive ? zeroY - h : zeroY,
      width: barW,
      height: h,
      positive,
    };
  });
}
