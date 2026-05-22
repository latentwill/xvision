import type { NavigateFunction } from "react-router-dom";

import type { InlineAction } from "@/api/chat_rail";
import { useUi } from "@/stores/ui";

export function runInlineAction(action: InlineAction, navigate: NavigateFunction) {
  if (action.href?.startsWith("/")) {
    navigate(action.href);
    return;
  }

  switch (action.command) {
    case "open_command_palette":
      useUi.getState().setCmdkOpen(true);
      return;
    case "start_eval":
      navigate("/eval-runs?start=1");
      return;
    case "create_strategy":
      navigate("/strategies/new");
      return;
    case "compare_runs":
      navigate("/eval-runs");
      return;
    case "open_settings":
      navigate("/settings");
      return;
  }
}
