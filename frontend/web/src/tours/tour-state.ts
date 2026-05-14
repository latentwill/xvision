import {
  safeStorageGet,
  safeStorageRemove,
  safeStorageSet,
} from "@/lib/storage";

const COMPLETED_PREFIX = "xvn.tours.completed.";

export function tourStorageKey(tourId: string): string {
  return `${COMPLETED_PREFIX}${tourId}`;
}

export function isTourCompleted(tourId: string): boolean {
  return safeStorageGet(tourStorageKey(tourId)) === "1";
}

export function markTourCompleted(tourId: string) {
  safeStorageSet(tourStorageKey(tourId), "1");
}

export function clearTourCompletion(tourId: string) {
  safeStorageRemove(tourStorageKey(tourId));
}
