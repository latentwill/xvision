export function isInflightRunStatus(status: string): boolean {
  return status === "queued" || status === "running";
}
