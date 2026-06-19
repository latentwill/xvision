// Full-screen login route. Shown when a non-loopback client hits a
// mutating route without a valid dashboard password cookie.
//
// Flow:
//   1. On mount, check if a dashboard password is configured via
//      GET /api/settings/dashboard-auth.
//   2. If no password is set → dashboard is open, redirect to home.
//   3. If password IS set → show password form.
//   4. On submit → POST /api/auth/login. On success → redirect to `next`.
//
// No env var required. The operator sets a password via Settings UI.

import { useEffect, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { login } from "@/api/auth";
import { apiFetch } from "@/api/client";
import { BrandMark } from "@/components/primitives/BrandMark";

interface AuthStatus {
  password_set: boolean;
}

export function LoginRoute() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const next = searchParams.get("next") ?? "/";

  const [password, setPassword] = useState("");
  const [loading, setLoading] = useState(true); // true during initial check
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Check if a password is configured.
  useEffect(() => {
    let cancelled = false;
    async function check() {
      try {
        const status = await apiFetch<AuthStatus>("/api/settings/dashboard-auth");
        if (cancelled) return;
        if (!status.password_set) {
          // Dashboard is open — no login needed.
          navigate(next, { replace: true });
          return;
        }
      } catch {
        // Keep the login form visible if the status check fails.
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    check();
    return () => { cancelled = true; };
  }, [navigate, next]);

  async function handleLogin(e: React.FormEvent) {
    e.preventDefault();
    if (!password.trim()) return;
    setSubmitting(true);
    setError(null);
    try {
      const result = await login(password);
      if (result.ok) {
        navigate(next, { replace: true });
      } else {
        setError(result.message);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Login failed — check server logs.");
    } finally {
      setSubmitting(false);
    }
  }

  if (loading) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-surface">
        <div className="text-[13px] text-text-3">Checking auth status…</div>
      </div>
    );
  }

  return (
    <div className="min-h-screen flex flex-col items-center justify-center bg-surface px-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="space-y-3 text-center flex flex-col items-center">
          <BrandMark height={28} />
          <p className="text-[13px] text-text-3">
            This dashboard is password-protected.
          </p>
        </div>

        <form onSubmit={handleLogin} className="rounded-lg border border-border bg-surface-elev p-6 space-y-4">
          <div>
            <label
              htmlFor="password"
              className="block text-[12px] font-medium text-text-2 mb-1.5"
            >
              Dashboard password
            </label>
            <input
              id="password"
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              autoFocus
              autoComplete="current-password"
              className="w-full rounded border border-border bg-surface px-3 py-2 text-[13px] text-text placeholder:text-text-3 focus:outline-none focus:border-gold"
              placeholder="Enter password"
            />
          </div>

          {error ? (
            <div className="rounded border border-danger/40 bg-danger/10 px-3 py-2 text-[12px] text-danger font-mono">
              {error}
            </div>
          ) : null}

          <button
            type="submit"
            disabled={submitting || !password.trim()}
            className={[
              "w-full rounded px-4 py-2.5 text-[13px] font-medium transition-opacity",
              "bg-gold text-surface",
              submitting || !password.trim()
                ? "opacity-50 cursor-not-allowed"
                : "hover:opacity-90",
            ].join(" ")}
          >
            {submitting ? "Signing in…" : "Sign in"}
          </button>
        </form>

        <p className="text-center text-[11px] text-text-3">
          Set or change the password in Settings → Dashboard Auth.
        </p>
      </div>
    </div>
  );
}
