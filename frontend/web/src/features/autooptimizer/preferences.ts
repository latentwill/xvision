const MUTATOR_MODEL_KEY = "autooptimizer_mutator_model";
const JUDGE_MODEL_KEY = "autooptimizer_judge_model";

const LEGACY_MUTATOR_MODEL_KEY = "ar_mutator_model";
const LEGACY_JUDGE_MODEL_KEY = "ar_judge_model";

function getStoredValue(key: string, legacyKey: string): string | null {
  return localStorage.getItem(key) ?? localStorage.getItem(legacyKey);
}

function setStoredValue(key: string, legacyKey: string, value: string): void {
  localStorage.setItem(key, value);
  localStorage.removeItem(legacyKey);
}

export function getStoredMutatorModel(): string | null {
  return getStoredValue(MUTATOR_MODEL_KEY, LEGACY_MUTATOR_MODEL_KEY);
}

export function setStoredMutatorModel(value: string): void {
  setStoredValue(MUTATOR_MODEL_KEY, LEGACY_MUTATOR_MODEL_KEY, value);
}

export function getStoredJudgeModel(): string | null {
  return getStoredValue(JUDGE_MODEL_KEY, LEGACY_JUDGE_MODEL_KEY);
}

export function setStoredJudgeModel(value: string): void {
  setStoredValue(JUDGE_MODEL_KEY, LEGACY_JUDGE_MODEL_KEY, value);
}
