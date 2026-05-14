import type {
  InlineChartContentBlock,
  InlineChartSeries,
  InlinePoint,
  InlineTone,
} from "@/api/chat_rail";

export type {
  InlineChartContentBlock,
  InlineChartSeries,
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
