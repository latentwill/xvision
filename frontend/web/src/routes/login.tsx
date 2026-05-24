// Full-screen login route. Shown when a non-loopback client receives a 401
// from a mutating API call. No modal or sheet — this is a real route that
// occupies the whole viewport, keeping deep-link context via the `next`
// query parameter.
//
// The XVN_DASHBOARD_TOKEN env var controls whether a token is required;
// loopback clients never see this page. When the user is on a remote /
// Tailscale deployment and the server returns 401, the app redirects here.
//
// After a successful POST /api/auth/session, the session token is stored in
// the auth store (sessionStorage-backed) and the user is redirected back to
// the page they were trying to reach via `next`.

import { useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { createSession } from "@/api/auth";
import { useAuth } from "@/stores/auth";
import { ApiError } from "@/api/client";

export function LoginRoute() {
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const next = searchParams.get("next") ?? "/";

  const setSession = useAuth((s) => s.setSession);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleLogin() {
    setLoading(true);
    setError(null);
    try {
      const session = await createSession();
      setSession(session);
      // Replace so "back" doesn't loop to /login.
      navigate(next, { replace: true });
    } catch (err) {
      if (err instanceof ApiError) {
        setError(`${err.code}: ${err.message}`);
      } else if (err instanceof Error) {
        setError(err.message);
      } else {
        setError("Unknown error — check server logs.");
      }
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="min-h-screen flex flex-col items-center justify-center bg-surface px-4">
      <div className="w-full max-w-sm space-y-6">
        <div className="space-y-1 text-center">
          <h1 className="font-sans font-semibold text-[28px] tracking-tight text-text">
            xvision
          </h1>
          <p className="text-[13px] text-text-3">
            Dashboard access requires a session token.
          </p>
        </div>

        <div className="rounded-lg border border-border bg-surface-elev p-6 space-y-4">
          <p className="text-[13px] text-text-2 leading-relaxed">
            This dashboard is deployed in non-loopback mode. Click{" "}
            <strong>Start session</strong> to create a time-limited session
            token. The server validates that{" "}
            <code className="font-mono text-[12px] text-text bg-surface px-1 rounded">
              XVN_DASHBOARD_TOKEN
            </code>{" "}
            is set before issuing one.
          </p>

          {error ? (
            <div className="rounded border border-danger/40 bg-danger/10 px-3 py-2 text-[12px] text-danger font-mono">
              {error}
            </div>
          ) : null}

          <button
            onClick={handleLogin}
            disabled={loading}
            className={[
              "w-full rounded px-4 py-2.5 text-[13px] font-medium transition-opacity",
              "bg-gold text-surface",
              loading ? "opacity-50 cursor-not-allowed" : "hover:opacity-90",
            ].join(" ")}
          >
            {loading ? "Creating session…" : "Start session"}
          </button>
        </div>

        <p className="text-center text-[11px] text-text-3">
          Loopback clients (localhost) bypass this gate automatically.
        </p>
      </div>
    </div>
  );
}
