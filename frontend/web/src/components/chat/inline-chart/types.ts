import type {
  InlineChartContentBlock,
  InlineChartKind,
  InlineChartSeries,
  InlineMetric,
  InlinePoint,
  InlineTone,
} from "@/api/chat_rail";

export type {
  InlineChartContentBlock,
  InlineChartKind,
  InlineChartSeries,
  InlineMetric,
  InlinePoint,
  InlineTone,
};

export type ChartBounds = {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
};

export type ViewBox = {
  width: number;
  height: number;
  padX: number;
  padY: number;
};

export type NormalizedPoint = InlinePoint & {
  sx: number;
  sy: number;
};
