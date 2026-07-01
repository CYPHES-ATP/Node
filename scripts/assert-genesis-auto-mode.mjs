import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../src/lib/genesisAutoMode.ts", import.meta.url), "utf8");
const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");

const checks = [
  {
    label: "settings key isolates current boot settings from persisted v0.7.2 worker mode",
    pattern: /SETTINGS_KEY\s*=\s*"cyphes\.genesis-auto-mode\.settings\.v3"/,
  },
  {
    label: "boot read forces auto worker off",
    pattern: /readGenesisAutoModeSettings\(\)[\s\S]*autoWorker:\s*false/,
  },
  {
    label: "boot read forces quest seeder off",
    pattern: /readGenesisAutoModeSettings\(\)[\s\S]*questSeeder:\s*false/,
  },
  {
    label: "persisted settings store auto worker off for next launch",
    pattern: /writeGenesisAutoModeSettings\([\s\S]*autoWorker:\s*false/,
  },
  {
    label: "persisted settings store quest seeder off for next launch",
    pattern: /writeGenesisAutoModeSettings\([\s\S]*questSeeder:\s*false/,
  },
];

const appChecks = [
  {
    label: "campaign seed cap supports sustained testnet load",
    pattern: /MAX_AUTO_CAMPAIGNS_PER_DAY\s*=\s*2400/,
  },
];

const failures = [
  ...checks.filter((check) => !check.pattern.test(source)),
  ...appChecks.filter((check) => !check.pattern.test(appSource)),
];

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`Genesis auto-mode invariant failed: ${failure.label}`);
  }
  process.exit(1);
}
