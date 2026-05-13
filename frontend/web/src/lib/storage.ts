export function safeStorageGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function safeStorageSet(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Safari can reject storage access in private or restricted contexts.
  }
}

export function safeStorageRemove(key: string) {
  try {
    localStorage.removeItem(key);
  } catch {
    // Best effort only; blocked storage must not prevent app startup.
  }
}
