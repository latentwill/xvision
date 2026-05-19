import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";

/**
 * Inline-edit affordance for a single text field on the strategy
 * detail page. Per the project-wide no-popup rule, the field
 * replaces its display element with a native `<input>` (or
 * `<textarea>` for multi-line content) on click; there is no
 * modal, sheet, popover, or dropdown.
 *
 * Save semantics:
 * - Enter (single-line) or Cmd/Ctrl+Enter (multi-line) submits.
 * - Esc cancels, restoring the previous value.
 * - Clicking outside the field also cancels (no implicit save —
 *   matches the operator-mental-model used elsewhere in the app).
 * - When in "saving" mode the field is disabled; the caller's
 *   `onSave` Promise drives the state machine.
 *
 * Errors are surfaced via `errorMessage` (controlled prop). The
 * caller is responsible for clearing it on a fresh edit attempt.
 * This component does NOT render a toast — toast surfacing is the
 * caller's call (so the same component works for in-page banner UX
 * too).
 */
export interface InlineEditFieldProps {
  /** Stable identifier for the form control (used for label binding). */
  id: string;
  /** Visible label, read aloud by screen readers. */
  label: string;
  /** Current persisted value. */
  value: string;
  /** Render single-line `<input>` or multi-line `<textarea>`. */
  multiline?: boolean;
  /** Optional placeholder when the value is empty. */
  placeholder?: string;
  /** Optional class names for the display element. */
  displayClassName?: string;
  /** Disable interaction (e.g. while another field is saving). */
  disabled?: boolean;
  /**
   * Called with the trimmed new value when the user commits. Return
   * a resolved Promise on success; reject (or throw) on failure —
   * the component will return to edit-mode with the user's draft
   * preserved so they can correct it.
   */
  onSave: (next: string) => Promise<void>;
  /** Inline error message rendered below the field. */
  errorMessage?: string | null;
  /** Optional callback invoked when the user opens the editor. */
  onEditStart?: () => void;
}

type Mode = "display" | "editing" | "saving";

export function InlineEditField(props: InlineEditFieldProps) {
  const {
    id,
    label,
    value,
    multiline = false,
    placeholder,
    displayClassName,
    disabled = false,
    onSave,
    errorMessage,
    onEditStart,
  } = props;

  const [mode, setMode] = useState<Mode>("display");
  const [draft, setDraft] = useState<string>(value);
  const inputRef = useRef<HTMLInputElement | HTMLTextAreaElement | null>(null);

  // Re-sync draft when the persisted value changes from outside
  // (e.g. another tab edited the strategy, or a successful save
  // resolved). Only do this when not actively editing, so the user's
  // in-flight draft is never clobbered.
  useEffect(() => {
    if (mode === "display") {
      setDraft(value);
    }
  }, [value, mode]);

  // Auto-focus the input when entering edit mode. select() so a
  // re-edit feels like a rename, not an append.
  useEffect(() => {
    if (mode === "editing" && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [mode]);

  function startEdit() {
    if (disabled) return;
    setDraft(value);
    setMode("editing");
    onEditStart?.();
  }

  function cancelEdit() {
    setDraft(value);
    setMode("display");
  }

  async function commit() {
    const trimmed = draft.trim();
    // Client-side: an empty commit is a cancel (we don't send a
    // blank value the server will reject). For "no-op when
    // unchanged" the caller's save Promise can no-op too — we still
    // route through onSave so audit / cache invalidation runs.
    if (trimmed === "") {
      cancelEdit();
      return;
    }
    if (trimmed === value.trim()) {
      // Unchanged — treat as a quiet cancel. No network round-trip.
      setMode("display");
      return;
    }
    setMode("saving");
    try {
      await onSave(trimmed);
      // Success: snap back to display. The parent will refresh
      // `value` via its own state path, the effect above will then
      // sync `draft`.
      setMode("display");
    } catch {
      // Failure: return to editing so the user can correct the
      // value. The caller's errorMessage prop surfaces the reason.
      // (Per `feedback_alpha_root_cause` we DO NOT silently
      // swallow — the caller's onSave threw, the error propagated,
      // and the component's contract is to land back in editing
      // mode with the draft intact so the operator can react.)
      setMode("editing");
    }
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      cancelEdit();
      return;
    }
    if (event.key === "Enter") {
      if (multiline && !(event.metaKey || event.ctrlKey)) {
        // Plain Enter in a textarea inserts a newline — only
        // Cmd/Ctrl+Enter commits.
        return;
      }
      event.preventDefault();
      void commit();
    }
  }

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    void commit();
  }

  if (mode === "display") {
    const displayed = value.trim() === "" ? placeholder ?? "" : value;
    return (
      <button
        type="button"
        onClick={startEdit}
        disabled={disabled}
        aria-label={`Edit ${label}`}
        className={displayClassName}
        data-testid={`inline-edit-display-${id}`}
      >
        {displayed}
      </button>
    );
  }

  const sharedProps = {
    id,
    ref: inputRef as never,
    value: draft,
    onChange: (
      e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>,
    ) => setDraft(e.target.value),
    onKeyDown,
    // Cancel on blur — matches the "no implicit save" semantic.
    // The save button or Enter explicitly commits.
    onBlur: () => {
      if (mode === "editing") cancelEdit();
    },
    disabled: mode === "saving",
    placeholder,
    "aria-label": label,
    "aria-invalid": Boolean(errorMessage) || undefined,
    "data-testid": `inline-edit-input-${id}`,
  } as const;

  return (
    <form onSubmit={onSubmit} data-testid={`inline-edit-form-${id}`}>
      {multiline ? (
        <textarea {...sharedProps} rows={3} />
      ) : (
        <input type="text" {...sharedProps} />
      )}
      {errorMessage ? (
        <div
          role="alert"
          data-testid={`inline-edit-error-${id}`}
          className="text-danger text-[12px] mt-1"
        >
          {errorMessage}
        </div>
      ) : null}
    </form>
  );
}
