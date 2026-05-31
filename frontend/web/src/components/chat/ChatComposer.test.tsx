import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { ChatComposer } from "./ChatComposer";

function renderComposer(
  props: Partial<Parameters<typeof ChatComposer>[0]> = {},
) {
  const defaults = {
    value: "",
    placeholder: "Message",
    onChange: vi.fn(),
    onSubmit: vi.fn(),
    disabled: false,
  };

  return render(<ChatComposer {...defaults} {...props} />);
}

describe("ChatComposer", () => {
  it("does not submit whitespace via Enter key", async () => {
    const onSubmit = vi.fn();
    renderComposer({ value: "   ", onSubmit });

    await userEvent.type(screen.getByPlaceholderText("Message"), "{Enter}");

    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("disables send for blank input", async () => {
    const onSubmit = vi.fn();
    renderComposer({ value: "   ", onSubmit });

    const send = screen.getByRole("button", { name: "Send message" });
    expect(send).toBeDisabled();

    await userEvent.click(send);

    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("submits when the input has text", async () => {
    const onSubmit = vi.fn();
    renderComposer({ value: "hello", onSubmit });

    await userEvent.click(screen.getByRole("button", { name: "Send message" }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
  });

  it("switches to cancel while busy without submitting", async () => {
    const onCancel = vi.fn();
    const onSubmit = vi.fn();
    renderComposer({ busy: true, onCancel, onSubmit, value: "hello" });

    await userEvent.click(screen.getByRole("button", { name: "Stop response" }));

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("does not submit a new message from Enter while busy", async () => {
    const onCancel = vi.fn();
    const onSubmit = vi.fn();
    renderComposer({ busy: true, onCancel, onSubmit, value: "hello" });

    await userEvent.type(screen.getByPlaceholderText("Message"), "{Enter}");

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("disables send button while busy with no onCancel handler", () => {
    renderComposer({ busy: true, value: "hello" });

    expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
  });

  it("disables controls when disabled", () => {
    const onOpenActions = vi.fn();
    renderComposer({ disabled: true, onOpenActions, value: "hello" });

    expect(
      screen.getByRole("button", { name: "Open all functions" }),
    ).toBeDisabled();
    expect(screen.getByPlaceholderText("Message")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Send message" })).toBeDisabled();
  });

  it("renders and invokes the optional actions button", async () => {
    const onOpenActions = vi.fn();
    renderComposer({ onOpenActions });

    await userEvent.click(
      screen.getByRole("button", { name: "Open all functions" }),
    );

    expect(onOpenActions).toHaveBeenCalledTimes(1);
  });
});
