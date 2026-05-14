import type { NavigateFunction } from "react-router-dom";

import type { InlineAction } from "@/api/chat_rail";

export function runInlineAction(action: InlineAction, navigate: NavigateFunction) {
  if (action.href?.startsWith("/")) {
    navigate(action.href);
  }
}
