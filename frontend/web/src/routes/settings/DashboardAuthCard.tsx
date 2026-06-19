import { useState, useEffect } from "react";
import { Card } from "@/components/primitives/Card";
import { apiFetch, ApiError } from "@/api/client";

interface AuthStatus {
  password_set: boolean;
}

interface SetPasswordRequest {
  password: string | null;
  current_password?: string;
}

export function DashboardAuthCard() {
  const [passwordSet, setPasswordSet] = useState<boolean | null>(null);
  const [newPassword, setNewPassword] = useState("");
  const [currentPassword, setCurrentPassword] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);

  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const status = await apiFetch<AuthStatus>("/api/settings/dashboard-auth");
        if (!cancelled) setPasswordSet(status.password_set);
      } catch {
        if (!cancelled) setPasswordSet(null);
      }
    }
    load();
    return () => { cancelled = true; };
  }, []);

  async function handleSetPassword(e: React.FormEvent) {
    e.preventDefault();
    if (!newPassword.trim()) return;
    setSubmitting(true);
    setMessage(null);
    try {
      const body: SetPasswordRequest = { password: newPassword };
      if (passwordSet) {
        body.current_password = currentPassword || undefined;
      }
      const result = await apiFetch<AuthStatus>("/api/settings/dashboard-auth", {
        method: "PUT",
        body: JSON.stringify(body),
      });
      setPasswordSet(result.password_set);
      setNewPassword("");
      setCurrentPassword("");
      setMessage({ type: "success", text: "Password set. Dashboard is now protected." });
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : "Failed to set password";
      setMessage({ type: "error", text: msg });
    } finally {
      setSubmitting(false);
    }
  }

  async function handleClearPassword() {
    if (!currentPassword.trim()) return;
    setSubmitting(true);
    setMessage(null);
    try {
      const result = await apiFetch<AuthStatus>("/api/settings/dashboard-auth", {
        method: "PUT",
        body: JSON.stringify({ password: null, current_password: currentPassword }),
      });
      setPasswordSet(result.password_set);
      setNewPassword("");
      setCurrentPassword("");
      setMessage({ type: "success", text: "Password removed. Dashboard is now open." });
    } catch (err) {
      const msg = err instanceof ApiError ? err.message : "Failed to clear password";
      setMessage({ type: "error", text: msg });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Card className="p-5">
      <div className="mb-4">
        <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
          Dashboard password
        </h3>
        <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
          {passwordSet === null
            ? "Loading…"
            : passwordSet
              ? "A password is set. All mutating operations require authentication."
              : "No password set — the dashboard is open. Set one to restrict access."}
        </p>
      </div>

      <form onSubmit={handleSetPassword} className="space-y-4 max-w-md">
        {passwordSet ? (
          <div>
            <label
              htmlFor="currentPassword"
              className="block text-[12px] font-medium text-text-2 mb-1.5"
            >
              Current password
            </label>
            <input
              id="currentPassword"
              type="password"
              value={currentPassword}
              onChange={(e) => setCurrentPassword(e.target.value)}
              autoComplete="current-password"
              className="w-full rounded border border-border bg-surface px-3 py-2 text-[13px] text-text placeholder:text-text-3 focus:outline-none focus:border-gold"
              placeholder="Required to change or remove"
            />
          </div>
        ) : null}

        <div>
          <label
            htmlFor="newPassword"
            className="block text-[12px] font-medium text-text-2 mb-1.5"
          >
            {passwordSet ? "New password" : "Password"}
          </label>
          <input
            id="newPassword"
            type="password"
            value={newPassword}
            onChange={(e) => setNewPassword(e.target.value)}
            autoComplete="new-password"
            className="w-full rounded border border-border bg-surface px-3 py-2 text-[13px] text-text placeholder:text-text-3 focus:outline-none focus:border-gold"
            placeholder={passwordSet ? "Leave blank to keep current" : "Choose a password"}
          />
        </div>

        {message ? (
          <div
            className={`rounded border px-3 py-2 text-[12px] font-mono ${
              message.type === "success"
                ? "border-green-500/40 bg-green-500/10 text-green-400"
                : "border-danger/40 bg-danger/10 text-danger"
            }`}
          >
            {message.text}
          </div>
        ) : null}

        <div className="flex gap-3">
          <button
            type="submit"
            disabled={submitting || !newPassword.trim()}
            className={[
              "rounded px-4 py-2 text-[13px] font-medium transition-opacity",
              "bg-gold text-surface",
              submitting || !newPassword.trim()
                ? "opacity-50 cursor-not-allowed"
                : "hover:opacity-90",
            ].join(" ")}
          >
            {submitting ? "Saving…" : passwordSet ? "Change password" : "Set password"}
          </button>

          {passwordSet ? (
            <button
              type="button"
              onClick={handleClearPassword}
              disabled={submitting || !currentPassword.trim()}
              className={[
                "rounded px-4 py-2 text-[13px] font-medium transition-opacity",
                "border border-danger/40 text-danger",
                submitting || !currentPassword.trim()
                  ? "opacity-50 cursor-not-allowed"
                  : "hover:bg-danger/10",
              ].join(" ")}
            >
              Remove password
            </button>
          ) : null}
        </div>
      </form>
    </Card>
  );
}
