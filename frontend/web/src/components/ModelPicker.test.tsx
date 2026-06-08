import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ModelPicker } from "./ModelPicker";
import type { ProviderRow } from "@/api/types.gen";

function provider(overrides: Partial<ProviderRow> = {}): ProviderRow {
  return {
    name: "ollama",
    kind: "ollama",
    base_url: "http://localhost:11434",
    api_key_env: "",
    api_key_set: false,
    synthetic: false,
    is_default: false,
    enabled_models: ["llama3.2:latest", "qwen2.5-coder:7b"],
    ...overrides,
  };
}

describe("ModelPicker", () => {
  afterEach(() => cleanup());

  it("lists enabled Ollama models when the provider is no-auth", () => {
    const onChange = vi.fn();

    render(
      <ModelPicker
        rows={[provider()]}
        loading={false}
        provider="ollama"
        model=""
        filterProvider="ollama"
        onChange={onChange}
      />,
    );

    // Closed by default — options only mount once the trigger opens the menu.
    expect(screen.queryByRole("option")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /pick a model/i }));

    // (option names include the trailing "ollama" context-window label)
    expect(screen.getByRole("option", { name: /llama3\.2:latest/ })).toBeInTheDocument();
    const qwen = screen.getByRole("option", { name: /qwen2\.5-coder:7b/ });
    expect(qwen).toBeInTheDocument();

    fireEvent.click(qwen);
    expect(onChange).toHaveBeenCalledWith("ollama", "qwen2.5-coder:7b");
  });
});
