// Platform detection for keyboard hotkeys. Mac uses ⌘ (cmd); Linux/Windows
// use Ctrl. We branch on `navigator.platform` because the new
// `userAgentData.platform` isn't supported everywhere yet.

export function isMacPlatform(): boolean {
  if (typeof navigator === "undefined") return false;
  return navigator.platform.toLowerCase().includes("mac");
}

export function modKeyLabel(): string {
  return isMacPlatform() ? "⌘" : "Ctrl";
}
