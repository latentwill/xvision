import type { ReactNode } from "react";
import { Navigate } from "react-router-dom";

import { useViewportMode } from "@/components/responsive/useViewportMode";

import { ListCard, type ListColumn } from "./ListCard";
import { MListCard } from "./MListCard";
import type {
  ActiveFilter,
  ListSearchState,
  ListSortState,
} from "./useListState";

export type ResponsiveListCardProps<T> = {
  listId?: string;
  title?: ReactNode;
  count?: number;
  subtitle?: ReactNode;
  density?: "full" | "compact";
  toolbar: {
    search?: ListSearchState;
    filters?: ActiveFilter[];
    sort?: ListSortState;
    clearAll?: () => void;
  };
  columns?: ListColumn[];
  rows: T[];
  renderRow: (row: T, index: number) => ReactNode;
  renderMobileRow: (row: T, index: number) => ReactNode;
  rightAction?: ReactNode;
  actions?: ReactNode;
  footer?: ReactNode;
  loading?: boolean;
  error?: { message?: string; retry?: () => void } | null;
  empty?: ReactNode;
  emptyAction?: ReactNode;
  /** When set, the phone breakpoint redirects to this route instead of rendering MListCard. */
  mobileFallback?: { redirectTo: string };
};

export function ResponsiveListCard<T>(props: ResponsiveListCardProps<T>) {
  const mode = useViewportMode();

  if (mode === "phone") {
    if (props.mobileFallback) {
      return <Navigate to={props.mobileFallback.redirectTo} replace />;
    }
    return (
      <MListCard<T>
        title={props.title}
        count={props.count}
        subtitle={props.subtitle}
        rightAction={props.rightAction}
        toolbar={props.toolbar}
        rows={props.rows}
        renderRow={props.renderMobileRow}
        loading={props.loading}
        error={props.error}
        empty={props.empty}
        emptyAction={props.emptyAction}
      />
    );
  }

  return (
    <ListCard<T>
      listId={props.listId}
      title={props.title}
      count={props.count}
      subtitle={props.subtitle}
      density={props.density}
      toolbar={props.toolbar}
      columns={props.columns}
      rows={props.rows}
      renderRow={props.renderRow}
      actions={props.actions}
      footer={props.footer}
      loading={props.loading}
      error={props.error}
      empty={props.empty}
      emptyAction={props.emptyAction}
    />
  );
}
