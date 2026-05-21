// Dashboard session auth API.
//
// Endpoints:
//   POST   /api/auth/session          → create session, returns SessionResponse
//   GET    /api/auth/session/current   → validate token, returns CurrentSession
//   DELETE /api/auth/session           → revoke token, returns 204

import { apiFetch } from "./client";

export type SessionResponse = {
  token: string;
  session_id: string;
  expires_at: string;
};

export type CurrentSession = {
  session_id: string;
  expires_at: string;
  created_ip: string | null;
  label: string | null;
};

/// Create a new dashboard session. Returns the bearer token.
export async function createSession(): Promise<SessionResponse> {
  return apiFetch<SessionResponse>("/api/auth/session", {
    method: "POST",
    body: JSON.stringify({}),
  });
}

/// Fetch the current session info. Throws 401 ApiError if not authenticated.
export async function currentSession(token: string): Promise<CurrentSession> {
  return apiFetch<CurrentSession>("/api/auth/session/current", {
    headers: { authorization: `Bearer ${token}` },
  });
}

/// Revoke the current session.
export async function deleteSession(token: string): Promise<void> {
  await apiFetch<void>("/api/auth/session", {
    method: "DELETE",
    headers: { authorization: `Bearer ${token}` },
  });
}
