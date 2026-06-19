// Dashboard auth API.
//
// Endpoints:
//   POST   /api/auth/login              → verify password, set cookie
//   POST   /api/auth/session            → create session, returns SessionResponse
//   GET    /api/auth/session/current    → validate token, returns CurrentSession
//   DELETE /api/auth/session            → revoke token, returns 204

import { apiFetch } from "./client";

export type LoginResponse = {
  ok: boolean;
  password_set: boolean;
  message: string;
};

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

/// Verify the dashboard password and get a session cookie.
export async function login(password: string): Promise<LoginResponse> {
  return apiFetch<LoginResponse>("/api/auth/login", {
    method: "POST",
    body: JSON.stringify({ password }),
  });
}

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
