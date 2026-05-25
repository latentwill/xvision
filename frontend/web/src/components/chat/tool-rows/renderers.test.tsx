import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ToolRowView } from "./ToolRowView";
import {
  deniedRow,
  finishedRow,
  needsApprovalRow,
} from "./test-helpers";

describe("tool-row renderers — per-row content", () => {
  it("strategy create renders a create-strategy diff row", () => {
    const row = finishedRow("create_strategy", { output: "name: alpha" });
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("Create strategy")).toBeInTheDocument();
    expect(screen.getByText(/strategy create diff/i)).toBeInTheDocument();
    expect(screen.getByText(/name: alpha/)).toBeInTheDocument();
    expect(screen.getByText("done")).toBeInTheDocument();
  });

  it("update_manifest renders an update-strategy diff row", () => {
    const row = finishedRow("update_manifest");
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("Update strategy")).toBeInTheDocument();
    expect(screen.getByText(/strategy update diff/i)).toBeInTheDocument();
  });

  it("update_slot renders an agent-slot diff row", () => {
    const row = finishedRow("update_slot");
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("Update agent slot")).toBeInTheDocument();
    expect(screen.getByText(/agent slot diff/i)).toBeInTheDocument();
  });

  it("attach_agent renders an attach-agent row", () => {
    const row = finishedRow("attach_agent");
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("Attach agent")).toBeInTheDocument();
  });

  it("ab_compare renders an A/B compare result row", () => {
    const row = finishedRow("ab_compare", { output: "arm A wins" });
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("A/B compare")).toBeInTheDocument();
    expect(screen.getByText(/comparison result/i)).toBeInTheDocument();
    expect(screen.getByText(/arm A wins/)).toBeInTheDocument();
  });

  it("run_eval renders an eval-run status row with exit code", () => {
    const row = finishedRow("run_eval", { exitCode: 0 });
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("Eval run")).toBeInTheDocument();
    expect(screen.getByText(/eval run complete/i)).toBeInTheDocument();
  });

  it("run_optimizer renders optimizer progress", () => {
    const row = finishedRow("run_optimizer");
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("Optimizer")).toBeInTheDocument();
    expect(screen.getByText(/optimization complete/i)).toBeInTheDocument();
  });

  it("restore_checkpoint renders a checkpoint-restore row", () => {
    const row = finishedRow("restore_checkpoint");
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("Checkpoint restore")).toBeInTheDocument();
    expect(screen.getByText(/checkpoint restored/i)).toBeInTheDocument();
  });

  it("edit_focus_chain renders a focus-chain edit row", () => {
    const row = finishedRow("edit_focus_chain");
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("Focus chain edit")).toBeInTheDocument();
    expect(screen.getByText(/focus chain updated/i)).toBeInTheDocument();
  });

  it("unknown read-only tool renders the generic read-only fallback", () => {
    const row = finishedRow("some_new_reader", { sideEffect: "read_only" });
    render(<ToolRowView row={row} sideEffect="read_only" />);
    expect(screen.getByText("some_new_reader")).toBeInTheDocument();
    expect(screen.getByText("read-only tool")).toBeInTheDocument();
    expect(screen.getByText(/inspection only/i)).toBeInTheDocument();
  });

  it("unknown write tool renders the explicit unsupported-write row", () => {
    const row = finishedRow("some_new_writer", { sideEffect: "external_write" });
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText("some_new_writer")).toBeInTheDocument();
    expect(screen.getByText("unsupported write")).toBeInTheDocument();
    expect(
      screen.getByText(/cannot execute in Act mode until it is registered/i),
    ).toBeInTheDocument();
  });
});

describe("tool-row policy + denial states", () => {
  it("NeedsApproval shows an inline approve affordance that calls onApprove", async () => {
    const row = needsApprovalRow("create_strategy", { mode: "act" });
    const onApprove = vi.fn();
    render(
      <ToolRowView row={row} sideEffect="external_write" onApprove={onApprove} />,
    );

    expect(screen.getByText(/needs approval to run/i)).toBeInTheDocument();
    const btn = screen.getByRole("button", { name: /approve & run/i });
    await userEvent.click(btn);
    expect(onApprove).toHaveBeenCalledWith(row.spanId);
  });

  it("NeedsApproval with no handler disables the affordance (no popup)", () => {
    const row = needsApprovalRow("create_strategy");
    render(<ToolRowView row={row} sideEffect="external_write" />);
    const btn = screen.getByRole("button", { name: /approval required/i });
    expect(btn).toBeDisabled();
  });

  it("tool_denied shows the denial code + remediation", () => {
    const row = deniedRow("create_strategy", {
      code: "write_tool_in_research_mode",
      message: "Write tools are blocked in research mode.",
    });
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(screen.getByText(/Denied/)).toBeInTheDocument();
    expect(
      screen.getByText("write_tool_in_research_mode"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/blocked in research mode/i),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/switch the session to Act mode/i),
    ).toBeInTheDocument();
  });

  it("denied row does NOT show an approve affordance", () => {
    const row = deniedRow("create_strategy");
    render(<ToolRowView row={row} sideEffect="external_write" />);
    expect(
      screen.queryByRole("button", { name: /approve/i }),
    ).not.toBeInTheDocument();
  });
});
