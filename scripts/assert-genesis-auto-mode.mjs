import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../src/lib/genesisAutoMode.ts", import.meta.url), "utf8");
const appSource = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");

const checks = [
  {
    label: "frontend auto-mode state is scoped to the current testnet",
    pattern: /GENESIS_AUTO_MODE_TESTNET_ID\s*=\s*"cyphes-dev-v0\.7\.7"/,
  },
  {
    label: "settings key isolates current boot settings from prior testnets",
    pattern: /SETTINGS_KEY\s*=\s*`cyphes\.\$\{GENESIS_AUTO_MODE_TESTNET_ID\}\.genesis-auto-mode\.settings\.v1`/,
  },
  {
    label: "default boot remains verifier-only until Run",
    pattern: /DEFAULT_GENESIS_AUTO_MODE[\s\S]*autoWorker:\s*false[\s\S]*autoVerifier:\s*true[\s\S]*questSeeder:\s*false/,
  },
  {
    label: "boot read keeps verifier duty on",
    pattern: /readGenesisAutoModeSettings\(\)[\s\S]*autoVerifier:\s*true/,
  },
  {
    label: "boot read no longer forces auto worker off",
    pattern: /readGenesisAutoModeSettings\(\)[\s\S]*autoWorker:\s*false/,
    shouldMatch: false,
  },
  {
    label: "persisted settings no longer force quest seeder off",
    pattern: /writeGenesisAutoModeSettings\([\s\S]*questSeeder:\s*false/,
    shouldMatch: false,
  },
];

const appChecks = [
  {
    label: "campaign seed cap supports sustained testnet load",
    pattern: /MAX_AUTO_CAMPAIGNS_PER_DAY\s*=\s*2400/,
  },
];

const failures = [
  ...checks.filter((check) => check.pattern.test(source) !== (check.shouldMatch ?? true)),
  ...appChecks.filter((check) => check.pattern.test(appSource) !== (check.shouldMatch ?? true)),
];

if (failures.length > 0) {
  for (const failure of failures) {
    console.error(`Genesis auto-mode invariant failed: ${failure.label}`);
  }
  process.exit(1);
}
