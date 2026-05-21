// Session-token auth store.
//
// On the loopback / same-machine setup, the dashboard is accessible without
// a session token (the middleware exempts loopback clients). This store exists
// for non-loopback deployments (Tailscale, remote, team-shared instances)
// where `XVN_DASHBOARD_TOKEN` is set and the API returns 401 for unauthenticated
// mutating requests.
//
// The token is persisted to sessionStorage (not localStorage) so it is
// cleared when the tab is closed, matching the DEFAULT_SESSION_TTL_SECS = 86400
// server-side expiry window. Operators can extend persistence on their end by
// not closing the tab.

import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";

export type SessionInfo = {
  token: string;
  session_id: string;
  expires_at: string;
};

type AuthState = {
  session: SessionInfo | null;
  setSession: (session: SessionInfo) => void;
  clearSession: () => void;
  isAuthenticated: () => boolean;
};

export const useAuth = create<AuthState>()(
  persist(
    (set, get) => ({
      session: null,
      setSession: (session) => set({ session }),
      clearSession: () => set({ session: null }),
      isAuthenticated: () => {
        const s = get().session;
        if (!s) return false;
        // Treat as unauthenticated if token is past its expiry.
        try {
          const expiry = new Date(s.expires_at).getTime();
          if (Date.now() > expiry) {
            return false;
          }
        } catch {
          // Unparseable expiry — let the server be the authority.
        }
        return true;
      },
    }),
    {
      name: "xvn-auth-session",
      storage: createJSONStorage(() => sessionStorage),
    },
  ),
);

/// Extract the bearer token for use in Authorization headers.
/// Returns undefined when the user is not authenticated.
export function getSessionToken(): string | undefined {
  return useAuth.getState().session?.token ?? undefined;
}
