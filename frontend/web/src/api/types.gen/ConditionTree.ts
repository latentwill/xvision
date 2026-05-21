// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
// Mirrors `xvision_filters::ConditionTree`. serde `rename_all = snake_case`
// produces externally-tagged variant keys: `{ all: [...] }` / `{ any: [...] }`.

import type { Condition } from "./Condition";

export type ConditionTree =
  | { all: Array<Condition> }
  | { any: Array<Condition> };
