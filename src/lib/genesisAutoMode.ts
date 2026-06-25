export interface GenesisAutoModeSettings {
  autoWorker: boolean;
  autoVerifier: boolean;
  questSeeder: boolean;
  maxDailyWorkUnits: number;
  maxRuntimeMinutes: number;
  modelRequirement: "local-model-required" | "any-local-model";
}

export interface GenesisAutoCounters {
  date: string;
  workUnits: number;
  verifications: number;
  campaignsSeeded: number;
  targetCursor: number;
}

const SETTINGS_KEY = "cyphes.genesis-auto-mode.settings.v1";
const COUNTERS_KEY = "cyphes.genesis-auto-mode.counters.v1";

export const DEFAULT_GENESIS_AUTO_MODE: GenesisAutoModeSettings = {
  autoWorker: false,
  autoVerifier: false,
  questSeeder: false,
  maxDailyWorkUnits: 3,
  maxRuntimeMinutes: 8,
  modelRequirement: "local-model-required",
};

export function todayAutoModeKey(date = new Date()) {
  return date.toISOString().slice(0, 10);
}

export function defaultGenesisAutoCounters(): GenesisAutoCounters {
  return {
    date: todayAutoModeKey(),
    workUnits: 0,
    verifications: 0,
    campaignsSeeded: 0,
    targetCursor: 0,
  };
}

function readJson<T>(key: string, fallback: T): T {
  try {
    const value = window.localStorage.getItem(key);
    return value ? { ...fallback, ...(JSON.parse(value) as Partial<T>) } : fallback;
  } catch {
    return fallback;
  }
}

export function readGenesisAutoModeSettings() {
  return readJson(SETTINGS_KEY, DEFAULT_GENESIS_AUTO_MODE);
}

export function writeGenesisAutoModeSettings(settings: GenesisAutoModeSettings) {
  window.localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
}

export function normalizeGenesisAutoCounters(counters: GenesisAutoCounters) {
  const today = todayAutoModeKey();
  if (counters.date === today) return counters;
  return {
    ...defaultGenesisAutoCounters(),
    targetCursor: counters.targetCursor,
  };
}

export function readGenesisAutoCounters() {
  return normalizeGenesisAutoCounters(
    readJson(COUNTERS_KEY, defaultGenesisAutoCounters()),
  );
}

export function writeGenesisAutoCounters(counters: GenesisAutoCounters) {
  window.localStorage.setItem(COUNTERS_KEY, JSON.stringify(counters));
}
