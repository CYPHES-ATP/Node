export interface GenesisAutoModeSettings {
  autoWorker: boolean;
  autoVerifier: boolean;
  questSeeder: boolean;
  maxDailyObservations: number;
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
  targetsObserved: number;
  unchangedTargets: number;
}

export interface GuardianTargetObservation {
  targetId: string;
  lastObservedCommit?: string;
  lastObservedAt?: string;
  lastSeededCommit?: string;
  lastSeededAt?: string;
  lastError?: string;
  lastErrorAt?: string;
  unchangedCount: number;
}

export interface GuardianObservationLedger {
  version: string;
  targets: Record<string, GuardianTargetObservation>;
}

export const GENESIS_AUTO_MODE_TESTNET_ID = "cyphes-dev-v0.7.5";

const SETTINGS_KEY = `cyphes.${GENESIS_AUTO_MODE_TESTNET_ID}.genesis-auto-mode.settings.v1`;
const COUNTERS_KEY = `cyphes.${GENESIS_AUTO_MODE_TESTNET_ID}.genesis-auto-mode.counters.v1`;
const LEDGER_KEY = `cyphes.${GENESIS_AUTO_MODE_TESTNET_ID}.guardian-observation-ledger.v1`;

export const DEFAULT_GENESIS_AUTO_MODE: GenesisAutoModeSettings = {
  autoWorker: false,
  autoVerifier: true,
  questSeeder: false,
  maxDailyObservations: 2880,
  maxDailyWorkUnits: 2880,
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
    targetsObserved: 0,
    unchangedTargets: 0,
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
  const stored = readJson(SETTINGS_KEY, DEFAULT_GENESIS_AUTO_MODE);
  return {
    ...stored,
    autoWorker: false,
    autoVerifier: true,
    questSeeder: false,
    maxDailyObservations: Math.max(
      DEFAULT_GENESIS_AUTO_MODE.maxDailyObservations,
      stored.maxDailyObservations || DEFAULT_GENESIS_AUTO_MODE.maxDailyObservations,
    ),
    maxDailyWorkUnits: Math.max(
      DEFAULT_GENESIS_AUTO_MODE.maxDailyWorkUnits,
      stored.maxDailyWorkUnits || DEFAULT_GENESIS_AUTO_MODE.maxDailyWorkUnits,
    ),
    maxRuntimeMinutes: Math.max(1, stored.maxRuntimeMinutes || DEFAULT_GENESIS_AUTO_MODE.maxRuntimeMinutes),
  };
}

export function writeGenesisAutoModeSettings(settings: GenesisAutoModeSettings) {
  window.localStorage.setItem(
    SETTINGS_KEY,
    JSON.stringify({
      ...settings,
      autoWorker: false,
      autoVerifier: true,
      questSeeder: false,
    }),
  );
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

export function defaultGuardianObservationLedger(): GuardianObservationLedger {
  return {
    version: GENESIS_AUTO_MODE_TESTNET_ID,
    targets: {},
  };
}

export function readGuardianObservationLedger() {
  return readJson(LEDGER_KEY, defaultGuardianObservationLedger());
}

export function writeGuardianObservationLedger(ledger: GuardianObservationLedger) {
  window.localStorage.setItem(LEDGER_KEY, JSON.stringify(ledger));
}

export function recordGuardianObservation(
  ledger: GuardianObservationLedger,
  targetId: string,
  commitSha: string,
  seeded: boolean,
) {
  const current = ledger.targets[targetId] || {
    targetId,
    unchangedCount: 0,
  };
  const unchanged = current.lastObservedCommit === commitSha && !seeded;
  const next: GuardianTargetObservation = {
    ...current,
    targetId,
    lastObservedCommit: commitSha,
    lastObservedAt: new Date().toISOString(),
    lastError: undefined,
    lastErrorAt: undefined,
    unchangedCount: unchanged ? current.unchangedCount + 1 : current.unchangedCount,
  };
  if (seeded) {
    next.lastSeededCommit = commitSha;
    next.lastSeededAt = next.lastObservedAt;
  }
  const updated = {
    ...ledger,
    version: GENESIS_AUTO_MODE_TESTNET_ID,
    targets: {
      ...ledger.targets,
      [targetId]: next,
    },
  };
  writeGuardianObservationLedger(updated);
  return updated;
}

export function recordGuardianFailure(
  ledger: GuardianObservationLedger,
  targetId: string,
  error: string,
) {
  const current = ledger.targets[targetId] || {
    targetId,
    unchangedCount: 0,
  };
  const updated = {
    ...ledger,
    version: GENESIS_AUTO_MODE_TESTNET_ID,
    targets: {
      ...ledger.targets,
      [targetId]: {
        ...current,
        targetId,
        lastError: error.slice(0, 240),
        lastErrorAt: new Date().toISOString(),
      },
    },
  };
  writeGuardianObservationLedger(updated);
  return updated;
}
