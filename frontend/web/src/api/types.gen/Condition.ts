// Filter v1 frontend type — hand-typed pending Stage 3 ts-rs export.
// Mirrors `xvision_filters::Condition`.

import type { Operand } from "./Operand";
import type { Operator } from "./Operator";

export type Condition = { lhs: Operand; op: Operator; rhs: Operand };
