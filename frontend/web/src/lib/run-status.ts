export function isInflightRunStatus(status: string): boolean {
  return status === "queued" || status === "running";
}

export function isTerminalRunStatus(status: string): boolean {
  return (
    status === "completed" ||
    status === "failed" ||
    status === "cancelled" ||
    status === "disconnected"
  );
}

export function isRetryableRunStatus(status: string): boolean {
  return status === "failed" || status === "cancelled" || status === "disconnected";
}
