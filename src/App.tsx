import { listen } from "@tauri-apps/api/event";
import {
  type CSSProperties,
  type Dispatch,
  type SetStateAction,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Activity,
  Clock3,
  Cpu,
  Gauge,
  Play,
  ShieldCheck,
  Square,
  Target,
  Trophy,
} from "lucide-react";
import { TitleBar } from "@/components/layout/TitleBar";
import { P2PProvider } from "@/components/providers/P2PProvider";
import { useP2P } from "@/hooks/useP2P";
import {
  type GenesisAutoCounters,
  type GenesisAutoModeSettings,
  normalizeGenesisAutoCounters,
  readGuardianObservationLedger,
  readGenesisAutoCounters,
  readGenesisAutoModeSettings,
  recordGuardianFailure,
  recordGuardianObservation,
  type GuardianObservationLedger,
  writeGenesisAutoCounters,
  writeGenesisAutoModeSettings,
} from "@/lib/genesisAutoMode";
import { isTauriRuntime } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";
import type {
  AuditRuntimeProgress,
  CampaignReportSnapshot,
  GitHubAccessStatus,
  GuardianTarget,
  LocalModelList,
  NodeContribution,
  ProtocolAuditCampaign,
  RepositorySummary,
} from "@/types";

const AUDIT_SCOPE = [
  "Dependency and supply-chain risk",
  "Secrets, permissions, and exposed configuration",
  "CI workflow and repository security posture",
  "Prioritized findings with reproducible evidence",
];
const AUTO_TICK_INTERVAL_MS = 12_000;
const TELEMETRY_TICK_INTERVAL_MS = 1_000;
const MAX_AUTO_CAMPAIGNS_PER_DAY = 2400;
const MAX_SELF_PENDING_CONTRIBUTIONS = 25;
const PENDING_CONTRIBUTION_BASE_CREDIT = 35;
const PARSER_FALLBACK_PENDING_MULTIPLIER = 0.10;
const APP_VERSION = import.meta.env.VITE_APP_VERSION || "0.7.14";
const RUNTIME_PROVIDER_OPTIONS = ["lmstudio", "ollama"];

interface GitHubRepository {
  full_name: string;
  html_url: string;
  description: string | null;
  language: string | null;
  default_branch: string;
  stargazers_count: number;
  private: boolean;
  message?: string;
}

interface GitHubCommit {
  sha: string;
  message?: string;
}

interface GitHubInputTarget {
  apiUrl: string;
  repoUrl: string;
  kind: "repository" | "blob" | "tree";
  pathSegments: string[];
}

interface InspectedRepository {
  repository: RepositorySummary;
  scope: string[];
  scopeText: string;
  focusPath?: string;
  focusRef?: string;
}

interface CockpitEvent {
  id: string;
  label: string;
  at: number;
  tone?: "info" | "success" | "warn" | "danger";
}

interface NetworkProgressStats {
  totalWorkUnits: number;
  clearedWorkUnits: number;
  totalContributions: number;
  verifiedContributions: number;
  pendingContributions: number;
  independentlyVerifiablePendingContributions: number;
  selfPendingContributions: number;
  pendingGrossCredits: number;
  pendingPenaltyCredits: number;
  parserFallbackPendingContributions: number;
  workPercent: number;
  settlementPercent: number;
}

const GITHUB_REPOSITORY_URL_ERROR =
  "Use a public GitHub repository URL, file URL, or folder URL, for example https://github.com/owner/repo.";

function parseGitHubInput(value: string): GitHubInputTarget | null {
  let parsed: URL;
  try {
    parsed = new URL(value.trim());
  } catch {
    return null;
  }
  if (!/^https:$/.test(parsed.protocol) || !/^(www\.)?github\.com$/i.test(parsed.hostname)) {
    return null;
  }
  const segments = parsed.pathname
    .replace(/\/+$/, "")
    .split("/")
    .filter(Boolean)
    .map((segment) => {
      try {
        return decodeURIComponent(segment);
      } catch {
        return segment;
      }
    });
  if (segments.length < 2) return null;
  const owner = segments[0];
  const repo = segments[1].replace(/\.git$/i, "");
  if (!owner || !repo || owner === "." || repo === ".") return null;
  const route = segments[2]?.toLowerCase();
  const kind = route === "blob" || route === "tree" ? route : "repository";
  const pathSegments = kind === "repository" ? [] : segments.slice(3);
  return {
    apiUrl: `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}`,
    repoUrl: `https://github.com/${owner}/${repo}`,
    kind,
    pathSegments,
  };
}

function repositoryFocusScope(
  baseScope: string[],
  repository: RepositorySummary,
  focusPath?: string,
  focusRef?: string,
) {
  if (!focusPath || !focusRef) {
    return {
      scope: baseScope,
      scopeText: baseScope.join("\n"),
    };
  }
  const focusedScope = [
    `Focused path: ${focusPath}`,
    `GitHub ref from pasted URL: ${focusRef}`,
    `Pinned commit: ${repository.commitSha}`,
    ...baseScope,
  ];
  return {
    scope: focusedScope,
    scopeText: focusedScope.join("\n"),
  };
}

function toRepositorySummary(
  repository: GitHubRepository,
  commitSha: string,
): RepositorySummary {
  return {
    fullName: repository.full_name,
    url: repository.html_url,
    description: repository.description,
    language: repository.language,
    defaultBranch: repository.default_branch,
    stars: repository.stargazers_count,
    isPrivate: repository.private,
    commitSha,
  };
}

function scopeLine(scopeText: string, prefix: string) {
  return scopeText
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.toLowerCase().startsWith(prefix.toLowerCase()))
    ?.slice(prefix.length)
    .trim();
}

function campaignFocus(campaign: ProtocolAuditCampaign) {
  return (
    scopeLine(campaign.scopeText, "Focused path:") ||
    scopeLine(campaign.scopeText, "Focused file:") ||
    "Full repository"
  );
}

function compactAuditBrief(brief?: string | null) {
  const normalized = (brief || "CYPHES is coordinating signed audit work for this pinned public repository.")
    .replace(/\s+/g, " ")
    .trim();
  const sentences = normalized.match(/[^.!?]+[.!?]+/g);
  return sentences ? sentences.slice(0, 2).join(" ").trim() : normalized;
}

function githubPauseEventLabel(status: GitHubAccessStatus) {
  const until = status.retryAt ? ` until ${new Date(status.retryAt).toLocaleTimeString()}` : "";
  const reason = /rate limit/i.test(status.message) ? "rate limit" : status.message.replace(/\s+/g, " ").trim();
  return `GitHub paused${until}: ${reason}; add token for higher quota`;
}

function shortCommit(commitSha: string) {
  return commitSha ? commitSha.slice(0, 7).toUpperCase() : "UNPINNED";
}

function cockpitEventLabel(phase: string) {
  const normalized = phase.toLowerCase();
  if (normalized.includes("quality deduction") || normalized.includes("parser fallback")) return phase;
  if (normalized.includes("reading pinned github")) return "Fetched scoped files";
  if (normalized.includes("building model prompt")) return "Built audit prompt";
  if (normalized.includes("running local model")) return "Prompted local model";
  if (normalized.includes("parsing")) return "Parsed findings";
  if (normalized.includes("signing")) return "Signed contribution";
  if (normalized.includes("complete")) return "Audit skill complete";
  if (normalized.includes("preparing")) return "Loaded audit skill";
  return phase;
}

function cockpitEventTone(phase: string): CockpitEvent["tone"] {
  const normalized = phase.toLowerCase();
  if (normalized.includes("quality deduction") || normalized.includes("parser fallback")) return "danger";
  return "info";
}

function formatClock(ms: number) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

function percentComplete(done: number, total: number) {
  if (total <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((done / total) * 100)));
}

function formatCreditAmount(value: number) {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

function isParserFallbackContribution(contribution: NodeContribution) {
  if (contribution.findings.length > 0) return false;
  const notes = contribution.notesMarkdown || "";
  if (notes.includes("CYPHES parser note: model output was not valid structured JSON")) return true;
  if (contribution.commands?.some((command) => command.toLowerCase().includes("structured parse failed"))) {
    return true;
  }
  return Boolean(contribution.coverage?.some(
    (coverage) =>
      coverage.area.toLowerCase() === "local model output" &&
      coverage.status.toLowerCase() === "needs_review",
  ));
}

function pendingPenaltyForContribution(contribution: NodeContribution) {
  if (!isParserFallbackContribution(contribution)) return 0;
  return PENDING_CONTRIBUTION_BASE_CREDIT * (1 - PARSER_FALLBACK_PENDING_MULTIPLIER);
}

function repositoryFullNameFromGitHubUrl(value: string) {
  const target = parseGitHubInput(value);
  if (!target) return "";
  const segments = new URL(target.repoUrl).pathname.split("/").filter(Boolean);
  return segments.slice(0, 2).join("/");
}

function campaignIncludesGuardianTarget(campaign: ProtocolAuditCampaign, target: GuardianTarget) {
  const targetMarker = `Guardian target: ${target.targetId}`;
  if (campaign.auditBriefText?.includes(targetMarker)) return true;
  const fullName = repositoryFullNameFromGitHubUrl(target.repoUrl).toLowerCase();
  return (
    fullName.length > 0 &&
    campaign.repository.fullName.toLowerCase() === fullName &&
    campaign.scopeText.trim() === target.scopeText.trim()
  );
}

function isRecentGuardianFailure(observedAt?: string) {
  if (!observedAt) return false;
  const observedMs = Date.parse(observedAt);
  if (!Number.isFinite(observedMs)) return false;
  return Date.now() - observedMs < 24 * 60 * 60 * 1000;
}

function isGitHubBackoffError(message: string) {
  return /github paused|rate limit/i.test(message);
}

function requestedByLocalNode(campaign: ProtocolAuditCampaign, agentId: string) {
  return Boolean(agentId && campaign.requesterAgentId === agentId);
}

function updateAutoCounter(
  setAutoCounters: Dispatch<SetStateAction<GenesisAutoCounters>>,
  updater: (current: GenesisAutoCounters) => GenesisAutoCounters,
) {
  setAutoCounters((current) => {
    const next = updater(normalizeGenesisAutoCounters(current));
    writeGenesisAutoCounters(next);
    return next;
  });
}

function guardianEpochKey(targetCursor: number, targetCount: number) {
  const size = Math.max(1, targetCount);
  const round = Math.floor(Math.max(0, targetCursor) / size) + 1;
  return `epoch-${round}`;
}

function AppContent() {
  const p2p = useP2P();
  const [, setActionJobId] = useState<string | null>(null);
  const [runtimeProvider, setRuntimeProvider] = useState("lmstudio");
  const [runtimeModels, setRuntimeModels] = useState<string[]>([]);
  const [runtimeModel, setRuntimeModel] = useState("");
  const [, setRuntimeStatus] = useState<LocalModelList | null>(null);
  const [githubAccessStatus, setGitHubAccessStatus] = useState<GitHubAccessStatus | null>(null);
  const [, setRuntimeProgress] = useState<Record<string, AuditRuntimeProgress>>({});
  const [latestRuntimeProgress, setLatestRuntimeProgress] = useState<AuditRuntimeProgress | null>(null);
  const [latestRuntimeEventAt, setLatestRuntimeEventAt] = useState(Date.now());
  const [, setRuntimeStartedAt] = useState<number | null>(null);
  const [telemetryTick, setTelemetryTick] = useState(Date.now());
  const [, setRunningWorkUnitId] = useState<string | null>(null);
  const [cockpitEvents, setCockpitEvents] = useState<CockpitEvent[]>([
    { id: "boot", label: "Runtime standby", at: Date.now() },
  ]);
  const [guardianTargets, setGuardianTargets] = useState<GuardianTarget[]>([]);
  const [autoMode, setAutoMode] = useState<GenesisAutoModeSettings>(() =>
    readGenesisAutoModeSettings(),
  );
  const [autoCounters, setAutoCounters] = useState<GenesisAutoCounters>(() =>
    readGenesisAutoCounters(),
  );
  const [guardianLedger, setGuardianLedger] = useState<GuardianObservationLedger>(() =>
    readGuardianObservationLedger(),
  );
  const [, setAutoBusy] = useState(false);
  const [, setAutoPulse] = useState("Autonomous guardian loop starting.");
  const runtimeStartedAtRef = useRef<number | null>(null);
  const lastPhaseRef = useRef("");
  const autoBusyRef = useRef(false);
  const autoProviderFallbackRef = useRef(true);
  const lastAutoTickAtRef = useRef(0);
  const lastAutoPulseRef = useRef("");
  const lastGitHubPauseRef = useRef("");

  const nodeStatus = useCyphesStore((state) => state.nodeStatus);
  const nodeError = useCyphesStore((state) => state.nodeError);
  const agentId = useCyphesStore((state) => state.agentId);
  const peerCount = useCyphesStore((state) => state.peerCount);
  const networkInfo = useCyphesStore((state) => state.networkInfo);
  const campaigns = useCyphesStore((state) => state.campaigns);
  const creditSummary = useCyphesStore((state) => state.creditSummary);
  const notice = useCyphesStore((state) => state.notice);
  const setNotice = useCyphesStore((state) => state.setNotice);
  const [campaignSnapshots, setCampaignSnapshots] = useState<Record<string, CampaignReportSnapshot>>({});

  const runtimeProviderLabel = runtimeProvider === "ollama" ? "Ollama" : "LM Studio";
  const runtimeActive = Boolean(
    latestRuntimeProgress &&
    latestRuntimeProgress.progress > 0 &&
    latestRuntimeProgress.progress < 100,
  );
  const runtimeRecentlyFinished = Boolean(
    latestRuntimeProgress?.progress === 100 && telemetryTick - latestRuntimeEventAt < 8 * 60_000,
  );
  const currentProgress = latestRuntimeProgress?.progress || 0;
  const measuredTokensPerSecond = latestRuntimeProgress?.tokensPerSecond || 0;
  const currentTokensPerSecond = measuredTokensPerSecond;
  const hasPendingCredit = runtimeActive || runtimeRecentlyFinished;
  const pendingReceiptMeter = hasPendingCredit ? Math.min(35, Math.max(1, Math.round(currentProgress * 0.35))) : 0;
  const normalizedAutoCounters = normalizeGenesisAutoCounters(autoCounters);
  const networkProgress = useMemo<NetworkProgressStats>(() => {
    return Object.values(campaignSnapshots).reduce((stats, snapshot) => {
      const verified = new Set(snapshot.verifications.map((item) => item.targetContributionId));
      const totalContributions = snapshot.contributions.length;
      const verifiedContributions = snapshot.contributions.filter((item) => verified.has(item.contributionId)).length;
      const pendingContributions = snapshot.contributions.filter(
        (item) => !verified.has(item.contributionId),
      );
      const independentlyVerifiablePendingContributions = pendingContributions.filter(
        (item) => item.workerAgentId !== agentId,
      );
      const selfPendingContributions = pendingContributions.filter(
        (item) => item.workerAgentId === agentId,
      );
      const pendingPenaltyCredits = pendingContributions.reduce(
        (sum, contribution) => sum + pendingPenaltyForContribution(contribution),
        0,
      );
      const parserFallbackPendingContributions = pendingContributions.filter(isParserFallbackContribution).length;
      const clearedWorkUnits = snapshot.workUnits.filter(
        (unit) => !["open", "claimed"].includes(unit.status),
      ).length;
      const next = {
        totalWorkUnits: stats.totalWorkUnits + snapshot.workUnits.length,
        clearedWorkUnits: stats.clearedWorkUnits + clearedWorkUnits,
        totalContributions: stats.totalContributions + totalContributions,
        verifiedContributions: stats.verifiedContributions + verifiedContributions,
        pendingContributions: stats.pendingContributions + pendingContributions.length,
        independentlyVerifiablePendingContributions:
          stats.independentlyVerifiablePendingContributions +
          independentlyVerifiablePendingContributions.length,
        selfPendingContributions: stats.selfPendingContributions + selfPendingContributions.length,
        pendingGrossCredits: stats.pendingGrossCredits + pendingContributions.length * PENDING_CONTRIBUTION_BASE_CREDIT,
        pendingPenaltyCredits: stats.pendingPenaltyCredits + pendingPenaltyCredits,
        parserFallbackPendingContributions: stats.parserFallbackPendingContributions + parserFallbackPendingContributions,
        workPercent: 0,
        settlementPercent: 0,
      };
      return {
        ...next,
        workPercent: percentComplete(next.clearedWorkUnits, next.totalWorkUnits),
        settlementPercent: percentComplete(next.verifiedContributions, next.totalContributions),
      };
    }, {
      totalWorkUnits: 0,
      clearedWorkUnits: 0,
      totalContributions: 0,
      verifiedContributions: 0,
      pendingContributions: 0,
      independentlyVerifiablePendingContributions: 0,
      selfPendingContributions: 0,
      pendingGrossCredits: 0,
      pendingPenaltyCredits: 0,
      parserFallbackPendingContributions: 0,
      workPercent: 0,
      settlementPercent: 0,
    });
  }, [agentId, campaignSnapshots]);
  const pendingVerificationCount = networkProgress.independentlyVerifiablePendingContributions;
  const selfPendingVerificationCount = networkProgress.selfPendingContributions;
  const visibleProgress = runtimeActive || runtimeRecentlyFinished ? currentProgress : networkProgress.settlementPercent;
  const projectedPendingCredits = Math.max(
    0,
    networkProgress.pendingGrossCredits + pendingReceiptMeter - networkProgress.pendingPenaltyCredits,
  );
  const provisionalCreditTotal = creditSummary.provisionalTotal || 0;
  const activeNodeCount = nodeStatus === "online" ? peerCount + 1 : peerCount;
  const autoModeArmed = true;
  const workModeEnabled = autoMode.autoWorker || autoMode.questSeeder;
  const sortedCampaigns = useMemo(
    () => [...campaigns].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)),
    [campaigns],
  );
  const liveCampaign = useMemo(() => {
    if (latestRuntimeProgress) {
      const active = campaigns.find(
        (campaign) => campaign.campaignId === latestRuntimeProgress.campaignId,
      );
      if (active) return active;
    }
    return sortedCampaigns[0] || null;
  }, [campaigns, latestRuntimeProgress, sortedCampaigns]);
  const liveTarget = liveCampaign
    ? guardianTargets.find((target) => campaignIncludesGuardianTarget(liveCampaign, target))
    : null;
  const nextWatchTarget =
    guardianTargets.length > 0
      ? guardianTargets[normalizedAutoCounters.targetCursor % guardianTargets.length]
      : null;
  const watchObservation = nextWatchTarget
    ? guardianLedger.targets[nextWatchTarget.targetId]
    : undefined;

  function pushCockpitEvent(label: string, tone: CockpitEvent["tone"] = "info") {
    setCockpitEvents((current) => [
      {
        id: `${Date.now()}-${Math.random().toString(16).slice(2)}`,
        label,
        at: Date.now(),
        tone,
      },
      ...current,
    ].slice(0, 7));
  }

  function pushAutoPulse(label: string, tone: CockpitEvent["tone"] = "info") {
    setAutoPulse(label);
    if (lastAutoPulseRef.current === label) return;
    lastAutoPulseRef.current = label;
    pushCockpitEvent(label, tone);
  }

  async function refreshRuntimeModels(provider = runtimeProvider, allowProviderFallback = false) {
    let selectedProvider = provider;
    let listing = await p2p.listLocalModelModels(provider);
    if (allowProviderFallback && listing.models.length === 0) {
      for (const fallbackProvider of RUNTIME_PROVIDER_OPTIONS.filter((candidate) => candidate !== provider)) {
        const fallbackListing = await p2p.listLocalModelModels(fallbackProvider);
        if (fallbackListing.models.length === 0) continue;
        selectedProvider = fallbackProvider;
        listing = fallbackListing;
        setRuntimeProvider(fallbackProvider);
        break;
      }
    }
    setRuntimeStatus(listing);
    setRuntimeModels(listing.models);
    setRuntimeModel((current) => {
      if (current && listing.models.includes(current)) return current;
      return listing.models[0] || "";
    });
    if (selectedProvider !== provider) {
      pushAutoPulse(`Using ${listing.providerLabel}`, "info");
    }
    return listing;
  }

  async function enableWorkMode() {
    if (workModeEnabled) return;
    let selectedModel = runtimeModel;
    let availableModels = runtimeModels;
    if (!selectedModel || availableModels.length === 0) {
      try {
        const listing = await refreshRuntimeModels(runtimeProvider);
        availableModels = listing.models;
        if (selectedModel && !availableModels.includes(selectedModel)) {
          selectedModel = "";
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        pushAutoPulse(message, "warn");
        setNotice(message);
        return;
      }
    }
    if (!selectedModel && availableModels.length > 0) {
      selectedModel = availableModels[0];
      setRuntimeModel(selectedModel);
    }
    if (!selectedModel) {
      const message = `${runtimeProviderLabel} model required before Run.`;
      pushAutoPulse(message, "warn");
      setNotice(message);
      return;
    }
    setAutoMode((current) => ({
      ...current,
      autoVerifier: true,
      autoWorker: true,
      questSeeder: true,
    }));
    pushAutoPulse("Work mode enabled", "success");
    setNotice("CYPHES worker mode enabled; verifier duties remain active.");
  }

  function stopWorkMode() {
    setAutoMode((current) => ({
      ...current,
      autoVerifier: true,
      autoWorker: false,
      questSeeder: false,
    }));
    pushAutoPulse(runtimeActive ? "Worker mode stopping after current model run" : "Worker mode stopped", "warn");
    setNotice(
      runtimeActive
        ? "CYPHES will stop starting new local model work after the current run finishes."
        : "CYPHES worker mode stopped; verifier duties remain active.",
    );
  }

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    const cleanups: Array<() => void> = [];
    listen<AuditRuntimeProgress>("audit:runtime_progress", (event) => {
      if (disposed) return;
      const now = Date.now();
      if (!runtimeStartedAtRef.current || event.payload.progress <= 5) {
        runtimeStartedAtRef.current = now;
        setRuntimeStartedAt(now);
      }
      setLatestRuntimeEventAt(now);
      setLatestRuntimeProgress(event.payload);
      setRuntimeProgress((current) => ({
        ...current,
        [event.payload.campaignId]: event.payload,
      }));
      const phaseKey = `${event.payload.campaignId}:${event.payload.workUnitId}:${event.payload.phase}`;
      if (lastPhaseRef.current !== phaseKey) {
        lastPhaseRef.current = phaseKey;
        pushCockpitEvent(cockpitEventLabel(event.payload.phase), cockpitEventTone(event.payload.phase));
      }
    }).then((cleanup) => {
      cleanups.push(cleanup);
    });
    const eventListeners: Array<[string, string, CockpitEvent["tone"]]> = [
      ["audit:contribution_acknowledged", "Requester acknowledged receipt", "success"],
      ["audit:contribution_received", "Inbound contribution received", "success"],
      ["audit:verification_received", "Receipt verified; Verified ATP updated", "success"],
      ["audit:verification_acknowledged", "Credit receipt delivered", "success"],
      ["audit:verifier_liveness_resync", "Verifier resync requested", "warn"],
      ["p2p:peer_resync_requested", "Peer resync requested", "warn"],
      ["atp:delivery_failed", "Network delivery pending", "warn"],
    ];
    eventListeners.forEach(([eventName, label, tone]) => {
      listen(eventName, () => {
        if (!disposed) pushCockpitEvent(label, tone);
      }).then((cleanup) => {
        cleanups.push(cleanup);
      });
    });
    return () => {
      disposed = true;
      cleanups.forEach((cleanup) => cleanup());
    };
  }, []);

  useEffect(() => {
    if (!runtimeActive && !runtimeRecentlyFinished) return;
    const timer = window.setInterval(() => setTelemetryTick(Date.now()), TELEMETRY_TICK_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [runtimeActive, runtimeRecentlyFinished]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const allowFallback = autoProviderFallbackRef.current && !runtimeModel;
    autoProviderFallbackRef.current = false;
    void refreshRuntimeModels(runtimeProvider, allowFallback);
    const timer = window.setInterval(() => {
      void refreshRuntimeModels(runtimeProvider);
    }, 15_000);
    return () => window.clearInterval(timer);
  }, [runtimeProvider]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let disposed = false;
    async function refreshGitHubStatus() {
      try {
        const status = await p2p.getGitHubAccessStatus();
        if (!disposed) setGitHubAccessStatus(status);
      } catch {
        if (!disposed) {
          setGitHubAccessStatus({
            authenticated: false,
            paused: false,
            message: "GitHub status unavailable.",
          });
        }
      }
    }
    void refreshGitHubStatus();
    const timer = window.setInterval(() => {
      void refreshGitHubStatus();
    }, 15_000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    if (!githubAccessStatus?.paused) return;
    const label = githubPauseEventLabel(githubAccessStatus);
    if (lastGitHubPauseRef.current === label) return;
    lastGitHubPauseRef.current = label;
    pushCockpitEvent(label, "danger");
  }, [githubAccessStatus?.message, githubAccessStatus?.paused, githubAccessStatus?.retryAt]);

  useEffect(() => {
    p2p.listGuardianTargets()
      .then(setGuardianTargets)
      .catch((error) => {
        setAutoPulse(error instanceof Error ? error.message : String(error));
      });
  }, []);

  useEffect(() => {
    writeGenesisAutoModeSettings(autoMode);
  }, [autoMode]);

  useEffect(() => {
    const normalized = normalizeGenesisAutoCounters(autoCounters);
    if (normalized !== autoCounters) {
      setAutoCounters(normalized);
      writeGenesisAutoCounters(normalized);
    }
  }, [autoCounters]);

  useEffect(() => {
    if (!isTauriRuntime() || campaigns.length === 0) return;
    let disposed = false;
    async function refreshSnapshots() {
      const entries = await Promise.all(
        campaigns.map(async (campaign) => {
          try {
            return [campaign.campaignId, await p2p.getCampaignSnapshot(campaign.campaignId)] as const;
          } catch {
            return null;
          }
        }),
      );
      if (!disposed) {
        setCampaignSnapshots(
          Object.fromEntries(entries.filter((entry): entry is [string, CampaignReportSnapshot] => Boolean(entry))),
        );
      }
    }
    void refreshSnapshots();
    return () => {
      disposed = true;
    };
  }, [campaigns]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 5_000);
    return () => window.clearTimeout(timer);
  }, [notice, setNotice]);

  useEffect(() => {
    if (!isTauriRuntime() || !autoModeArmed) return;
    function runThrottledAutoTick() {
      const now = Date.now();
      if (now - lastAutoTickAtRef.current < AUTO_TICK_INTERVAL_MS) return;
      lastAutoTickAtRef.current = now;
      void runGenesisAutoTick();
    }
    runThrottledAutoTick();
    const timer = window.setInterval(() => {
      runThrottledAutoTick();
    }, AUTO_TICK_INTERVAL_MS);
    return () => window.clearInterval(timer);
  }, [
    agentId,
    autoMode,
    autoModeArmed,
    campaigns,
    campaignSnapshots,
    guardianLedger,
    guardianTargets,
    normalizedAutoCounters,
    runtimeModel,
    runtimeProvider,
  ]);

  async function resolveCommit(apiUrl: string, ref: string, optional = false) {
    const commitResponse = await fetch(
      `${apiUrl}/commits/${encodeURIComponent(ref)}`,
      {
        headers: {
          Accept: "application/vnd.github+json",
        },
      },
    );
    const commit = (await commitResponse.json()) as GitHubCommit;
    if (!commitResponse.ok) {
      if (optional && commitResponse.status === 404) return null;
      throw new Error(commit.message || `GitHub could not resolve ${ref} to a commit.`);
    }
    if (!/^[0-9a-f]{40,64}$/i.test(commit.sha || "")) {
      throw new Error(commit.message || `GitHub returned an invalid commit for ${ref}.`);
    }
    return commit.sha;
  }

  async function resolveGitHubPath(
    apiUrl: string,
    repository: GitHubRepository,
    target: GitHubInputTarget,
  ) {
    if (target.kind === "repository") {
      return {
        commitSha: (await resolveCommit(apiUrl, repository.default_branch))!,
        focusPath: undefined,
        focusRef: undefined,
      };
    }

    if (target.kind === "blob" && target.pathSegments.length < 2) {
      throw new Error("That GitHub file URL is missing a branch or path.");
    }
    if (target.pathSegments.length === 0) {
      return {
        commitSha: (await resolveCommit(apiUrl, repository.default_branch))!,
        focusPath: undefined,
        focusRef: undefined,
      };
    }

    const defaultBranchSegments = repository.default_branch.split("/");
    const startsWithDefaultBranch = defaultBranchSegments.every(
      (segment, index) => target.pathSegments[index] === segment,
    );
    if (startsWithDefaultBranch) {
      const focusPath = target.pathSegments.slice(defaultBranchSegments.length).join("/");
      return {
        commitSha: (await resolveCommit(apiUrl, repository.default_branch))!,
        focusPath: focusPath || undefined,
        focusRef: repository.default_branch,
      };
    }

    const maxRefSegments =
      target.kind === "blob"
        ? Math.max(1, target.pathSegments.length - 1)
        : target.pathSegments.length;
    for (let index = maxRefSegments; index >= 1; index -= 1) {
      const focusRef = target.pathSegments.slice(0, index).join("/");
      const commitSha = await resolveCommit(apiUrl, focusRef, true);
      if (commitSha) {
        const focusPath = target.pathSegments.slice(index).join("/");
        return {
          commitSha,
          focusPath: focusPath || undefined,
          focusRef,
        };
      }
    }

    throw new Error(
      "GitHub resolved the repository, but CYPHES could not resolve the branch or file path from that URL.",
    );
  }

  async function inspectRepository(url: string): Promise<InspectedRepository> {
    if (isTauriRuntime()) {
      const inspected = await p2p.inspectGithubRepository(url);
      return {
        repository: inspected.repository,
        ...repositoryFocusScope(
          AUDIT_SCOPE,
          inspected.repository,
          inspected.focusPath,
          inspected.focusRef,
        ),
        focusPath: inspected.focusPath,
        focusRef: inspected.focusRef,
      };
    }

    const target = parseGitHubInput(url);
    if (!target) {
      throw new Error(GITHUB_REPOSITORY_URL_ERROR);
    }

    const response = await fetch(target.apiUrl, {
      headers: {
        Accept: "application/vnd.github+json",
      },
    });
    const repository = (await response.json()) as GitHubRepository;

    if (!response.ok) {
      throw new Error(repository.message || "GitHub could not find that public repository.");
    }
    if (repository.private) {
      throw new Error("Private repositories are not supported in this build.");
    }

    const resolved = await resolveGitHubPath(target.apiUrl, repository, target);
    const summary = toRepositorySummary(
      repository,
      resolved.commitSha,
    );
    return {
      repository: summary,
      ...repositoryFocusScope(
        AUDIT_SCOPE,
        summary,
        resolved.focusPath,
        resolved.focusRef,
      ),
      focusPath: resolved.focusPath,
      focusRef: resolved.focusRef,
    };
  }

  function selectNextGuardianTarget() {
    if (guardianTargets.length === 0) return null;
    const startCursor = normalizedAutoCounters.targetCursor;
    for (let offset = 0; offset < guardianTargets.length; offset += 1) {
      const index = (startCursor + offset) % guardianTargets.length;
      const target = guardianTargets[index];
      const observation = guardianLedger.targets[target.targetId];
      if (isRecentGuardianFailure(observation?.lastErrorAt)) continue;
      return {
        target,
        nextCursor: startCursor + offset + 1,
      };
    }
    return null;
  }

  async function seedGuardianCampaign(target: GuardianTarget, nextCursor: number) {
    pushAutoPulse(`Watching ${target.protocolName}`, "info");
    const inspected = await inspectRepository(target.repoUrl);
    const currentEpoch = guardianEpochKey(normalizedAutoCounters.targetCursor, guardianTargets.length);
    const epochScopeLine = `Guardian epoch: ${currentEpoch}`;
    const existingCampaign = campaigns.find(
      (campaign) =>
        requestedByLocalNode(campaign, agentId) &&
        campaignIncludesGuardianTarget(campaign, target) &&
        campaign.repository.commitSha === inspected.repository.commitSha &&
        campaign.scopeText.includes(epochScopeLine),
    );
    const observation = guardianLedger.targets[target.targetId];
    const alreadySeededCommit =
      (observation?.lastSeededCommit === inspected.repository.commitSha &&
        observation?.lastSeededEpoch === currentEpoch) ||
      Boolean(existingCampaign);
    if (alreadySeededCommit) {
      const nextLedger = recordGuardianObservation(
        guardianLedger,
        target.targetId,
        inspected.repository.commitSha,
        false,
      );
      setGuardianLedger(nextLedger);
      updateAutoCounter(setAutoCounters, (current) => ({
        ...current,
        targetCursor: nextCursor,
        targetsObserved: current.targetsObserved + 1,
        unchangedTargets: current.unchangedTargets + 1,
      }));
      pushAutoPulse(`${target.protocolName} unchanged at ${shortCommit(inspected.repository.commitSha)}`, "info");
      return false;
    }
    const campaign = await p2p.createProtocolCampaign(
      inspected.repository,
      target.protocolName,
      [target.scopeText, epochScopeLine].join("\n\n"),
      target.creditBudget.toString(),
      [
        `Guardian target: ${target.targetId}`,
        epochScopeLine,
        target.auditBrief,
        `Category: ${target.category}`,
        `Chains: ${target.chains.join(", ")}`,
        `Priority score: ${target.priorityScore}`,
        `Static TVL/risk rank seed: ${target.tvlRiskRank}`,
        "Autonomous Guardian Loop: no external report submission, payout claim, or protocol contact without human approval.",
      ].join("\n\n"),
      [
        `Guardian target index tags: ${target.tags.join(", ")}`,
        `Cadence: ${target.cadence}`,
        target.docsUrl ? `Docs: ${target.docsUrl}` : "",
        target.securityUrl ? `Security reference: ${target.securityUrl}` : "",
        target.inScopeText ? `In scope: ${target.inScopeText}` : "",
        target.outOfScopeText ? `Out of scope: ${target.outOfScopeText}` : "",
      ].filter(Boolean).join("\n"),
      "",
    );
    const nextLedger = recordGuardianObservation(
      guardianLedger,
      target.targetId,
      inspected.repository.commitSha,
      true,
      currentEpoch,
    );
    setGuardianLedger(nextLedger);
    updateAutoCounter(setAutoCounters, (current) => ({
      ...current,
      campaignsSeeded: current.campaignsSeeded + 1,
      targetsObserved: current.targetsObserved + 1,
      targetCursor: nextCursor,
    }));
    await refreshCampaignSnapshot(campaign.campaignId);
    pushAutoPulse(`Work created: ${target.protocolName} ${shortCommit(inspected.repository.commitSha)}`, "success");
    setNotice(`CYPHES created ${target.protocolName} guardian work at ${shortCommit(inspected.repository.commitSha)}.`);
    return true;
  }

  function skipGuardianTargetForNow(target: GuardianTarget, nextCursor: number, message: string) {
    const nextLedger = recordGuardianFailure(guardianLedger, target.targetId, message);
    setGuardianLedger(nextLedger);
    updateAutoCounter(setAutoCounters, (current) => ({
      ...current,
      targetCursor: nextCursor,
      targetsObserved: current.targetsObserved + 1,
    }));
    pushAutoPulse(`Skipped ${target.protocolName}: ${message}`, "warn");
  }

  async function autoVerifyNextContribution() {
    for (const campaign of campaigns) {
      const snapshot = await refreshCampaignSnapshot(campaign.campaignId);
      const verifiedIds = new Set(snapshot.verifications.map((item) => item.targetContributionId));
      const pending = snapshot.contributions.filter(
        (item) =>
          !verifiedIds.has(item.contributionId) &&
          item.workerAgentId !== agentId,
      );
      const selfPending = snapshot.contributions.filter(
        (item) => !verifiedIds.has(item.contributionId) && item.workerAgentId === agentId,
      );
      if (pending.length === 0) {
        if (selfPending.length > 0) {
          pushAutoPulse("Awaiting independent verifier", "warn");
          setNotice("Signed contribution submitted; Verified ATP requires an independent network verifier.");
          return false;
        }
        continue;
      }
      pushAutoPulse(`Network verifier checking ${campaign.protocolName}`, "info");
      const issued: number[] = [];
      for (const contribution of pending) {
        const credits = await p2p.verifyCampaignContribution(
          contribution.contributionId,
          "accepted",
          "NETWORK_SIGNED_RECEIPT_ACCEPTED",
          "Independent network verifier accepted a signed contribution receipt for ATP settlement.",
        );
        issued.push(...credits.map((credit) => credit.total));
      }
      await refreshCampaignSnapshot(campaign.campaignId);
      updateAutoCounter(setAutoCounters, (current) => ({
        ...current,
        verifications: current.verifications + pending.length,
      }));
      const total = issued.reduce((sum, value) => sum + value, 0);
      pushAutoPulse(`Network verifier issued +${total} ATP`, "success");
      setNotice(`${pending.length} contribution${pending.length === 1 ? "" : "s"} network-verified; ${total} ATP Credits issued.`);
      return true;
    }
    if (pendingVerificationCount > 0) {
      pushAutoPulse("Verification pool available", "info");
    }
    return false;
  }

  async function autoRunWorkUnit(campaign: ProtocolAuditCampaign, workUnitId: string) {
    const actionId = `${campaign.campaignId}:${workUnitId}`;
    setActionJobId(actionId);
    setRunningWorkUnitId(workUnitId);
    try {
      const contribution = await p2p.runClaimedWorkUnit(
        campaign.campaignId,
        workUnitId,
        runtimeProvider,
        runtimeModel,
        autoMode.maxRuntimeMinutes * 60,
      );
      await refreshCampaignSnapshot(campaign.campaignId);
      updateAutoCounter(setAutoCounters, (current) => ({
        ...current,
        workUnits: current.workUnits + 1,
      }));
      pushAutoPulse("Auto worker submitted signed contribution", "success");
      setNotice(`CYPHES submitted ${campaign.protocolName}; receipt ${contribution.receiptHash.slice(0, 19)}... awaiting requester verification.`);
      return true;
    } finally {
      setRunningWorkUnitId(null);
      setActionJobId(null);
    }
  }

  async function autoWorkerNextUnit() {
    if (!runtimeModel) {
      pushAutoPulse(`Waiting for ${runtimeProviderLabel}`, "warn");
      return false;
    }
    if (normalizedAutoCounters.workUnits >= autoMode.maxDailyWorkUnits) {
      pushAutoPulse(`Model audit cap reached (${autoMode.maxDailyWorkUnits}/day)`, "warn");
      return false;
    }

    for (const campaign of campaigns) {
      const snapshot =
        campaignSnapshots[campaign.campaignId] ||
        (await refreshCampaignSnapshot(campaign.campaignId));
      const myClaim = snapshot.claims.find((claim) => {
        const hasContribution = snapshot.contributions.some(
          (contribution) =>
            contribution.workUnitId === claim.workUnitId &&
            contribution.workerAgentId === agentId,
        );
        return claim.workerAgentId === agentId && claim.status === "claimed" && !hasContribution;
      });
      if (myClaim) {
        pushAutoPulse("Running claimed unit", "info");
        return autoRunWorkUnit(campaign, myClaim.workUnitId);
      }
      const openUnit = snapshot.workUnits.find((unit) => unit.status === "open");
      if (!openUnit) continue;
      const actionId = `${campaign.campaignId}:${openUnit.workUnitId}`;
      setActionJobId(actionId);
      try {
        pushAutoPulse(`Claiming ${openUnit.title}`, "info");
        await p2p.claimCampaignWorkUnit(campaign.campaignId, openUnit.workUnitId);
        await refreshCampaignSnapshot(campaign.campaignId);
      } finally {
        setActionJobId(null);
      }
      pushAutoPulse(`Running ${openUnit.title}`, "info");
      return autoRunWorkUnit(campaign, openUnit.workUnitId);
    }
    pushAutoPulse("Scanning for open work", "info");
    return false;
  }

  async function runGenesisAutoTick() {
    if (!isTauriRuntime() || !agentId || autoBusyRef.current || !autoModeArmed) return;
    autoBusyRef.current = true;
    setAutoBusy(true);
    try {
      if (autoMode.autoVerifier) {
        const verified = await autoVerifyNextContribution();
        if (verified) return;
      }
      if (pendingVerificationCount > 0) {
        pushAutoPulse(`Verifier duty active: ${pendingVerificationCount} receipt${pendingVerificationCount === 1 ? "" : "s"} pending`, "warn");
        return;
      }
      if (selfPendingVerificationCount >= MAX_SELF_PENDING_CONTRIBUTIONS) {
        pushAutoPulse(`Awaiting verifier: ${selfPendingVerificationCount} submitted receipts pending`, "warn");
        setNotice(`CYPHES paused new audit work while ${selfPendingVerificationCount} submitted receipt${selfPendingVerificationCount === 1 ? "" : "s"} await independent verification.`);
        return;
      }
      if (!workModeEnabled) {
        pushAutoPulse("Verifier mode active", "info");
        return;
      }
      if (autoMode.autoWorker) {
        const worked = await autoWorkerNextUnit();
        if (worked) return;
      }
      if (autoMode.questSeeder) {
        if (normalizedAutoCounters.targetsObserved >= autoMode.maxDailyObservations) {
          pushAutoPulse(`Observation cap reached (${autoMode.maxDailyObservations}/day)`, "warn");
          return;
        }
        if (normalizedAutoCounters.campaignsSeeded >= MAX_AUTO_CAMPAIGNS_PER_DAY) {
          pushAutoPulse(`Campaign seed cap reached (${MAX_AUTO_CAMPAIGNS_PER_DAY}/day)`, "warn");
          return;
        }
        const selection = selectNextGuardianTarget();
        if (selection) {
          try {
            await seedGuardianCampaign(selection.target, selection.nextCursor);
          } catch (error) {
            const message = error instanceof Error ? error.message : String(error);
            if (isGitHubBackoffError(message)) throw error;
            skipGuardianTargetForNow(selection.target, selection.nextCursor, message);
          }
        }
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      pushAutoPulse(message, "warn");
      setNotice(message);
    } finally {
      autoBusyRef.current = false;
      setAutoBusy(false);
    }
  }

  async function refreshCampaignSnapshot(campaignId: string) {
    const snapshot = await p2p.getCampaignSnapshot(campaignId);
    setCampaignSnapshots((current) => ({ ...current, [campaignId]: snapshot }));
    return snapshot;
  }

  return (
    <div className="app-shell">
      <TitleBar />

      <main>
        <section className="panel intelligence-panel">
          {liveCampaign ? (
            <article className="intelligence-card">
              <div className="intelligence-topline">
                <span>{liveCampaign.status}</span>
                <code>{liveCampaign.repository.fullName}</code>
              </div>
              <div className="intelligence-main">
                <div>
                  <h3>{liveCampaign.protocolName}</h3>
                  <p className="audit-brief">{compactAuditBrief(liveTarget?.auditBrief)}</p>
                </div>
                <div className="runtime-radar">
                  <label>
                    <span>Provider</span>
                    <select
                      onChange={(event) => setRuntimeProvider(event.currentTarget.value)}
                      value={runtimeProvider}
                    >
                      <option value="lmstudio">LM Studio</option>
                      <option value="ollama">Ollama</option>
                    </select>
                  </label>
                  <label>
                    <span>Model</span>
                    <select
                      disabled={runtimeModels.length === 0}
                      onChange={(event) => setRuntimeModel(event.currentTarget.value)}
                      value={runtimeModel}
                    >
                      {runtimeModels.length === 0 ? (
                        <option value="">No model</option>
                      ) : (
                        runtimeModels.map((model) => (
                          <option key={model} value={model}>{model}</option>
                        ))
                      )}
                    </select>
                  </label>
                  <button
                    aria-label={workModeEnabled ? "Worker mode running" : "Run worker mode"}
                    className="runtime-run-button"
                    disabled={workModeEnabled || runtimeActive}
                    onClick={() => {
                      void enableWorkMode();
                    }}
                    type="button"
                  >
                    <Play size={14} aria-hidden="true" />
                    <span>{workModeEnabled ? "Running" : "Run"}</span>
                  </button>
                  <button
                    aria-label="Stop worker mode"
                    className="runtime-run-button runtime-stop-button"
                    disabled={!workModeEnabled && !runtimeActive}
                    onClick={stopWorkMode}
                    type="button"
                  >
                    <Square size={13} aria-hidden="true" />
                    <span>Stop</span>
                  </button>
                </div>
              </div>
              <div className="campaign-target">
                <span>{campaignFocus(liveCampaign)}</span>
                <code>{shortCommit(liveCampaign.repository.commitSha)}</code>
                {liveTarget ? <span>{liveTarget.chains.slice(0, 4).join(" / ")}</span> : null}
              </div>
              <div className="intelligence-grid telemetry-grid">
                <div>
                  <Gauge size={16} />
                  <small>Tokens/sec</small>
                  <strong>{currentTokensPerSecond.toFixed(1)}</strong>
                  <span>{runtimeActive ? "streaming local model" : measuredTokensPerSecond ? "last streamed run" : "waiting for model"}</span>
                </div>
                <div>
                  <Trophy size={16} />
                  <small>Verified ATP</small>
                  <strong>{creditSummary.total}</strong>
                  <span>{provisionalCreditTotal > 0 ? `${provisionalCreditTotal} provisional` : "independent receipts"}</span>
                </div>
                <div>
                  <Clock3 size={16} />
                  <small>Pending</small>
                  <strong className={networkProgress.pendingPenaltyCredits > 0 ? "pending-credit has-penalty" : "pending-credit"}>
                    <span>+{formatCreditAmount(networkProgress.pendingGrossCredits + pendingReceiptMeter)}</span>
                    {networkProgress.pendingPenaltyCredits > 0 ? (
                      <em>-{formatCreditAmount(networkProgress.pendingPenaltyCredits)}</em>
                    ) : null}
                  </strong>
                  <span>
                    {networkProgress.pendingPenaltyCredits > 0
                      ? `${formatCreditAmount(projectedPendingCredits)} expected after parser penalty`
                      : runtimeActive
                        ? `${currentProgress}% active`
                        : "awaiting receipts"}
                  </span>
                </div>
                <div>
                  <Activity size={16} />
                  <small>Active nodes</small>
                  <strong>{activeNodeCount}</strong>
                  <span>{networkInfo?.relay_connected ? "relay linked" : "network standby"}</span>
                </div>
              </div>
              <div className="terminal-progress intelligence-progress" aria-label="Audit skill progress">
                <span style={{ width: `${visibleProgress}%` } as CSSProperties} />
              </div>
              <div className="cockpit-events intelligence-events" aria-label="Live runtime event stream">
                {githubAccessStatus?.paused ? (
                  <div className="cockpit-event danger pinned" key="github-paused">
                    <time>HOLD</time>
                    <span>{githubPauseEventLabel(githubAccessStatus)}</span>
                  </div>
                ) : null}
                {cockpitEvents.map((event) => (
                  <div className={`cockpit-event ${event.tone || "info"}`} key={event.id}>
                    <time>{formatClock(telemetryTick - event.at)}</time>
                    <span>{event.label}</span>
                  </div>
                ))}
              </div>
              {latestRuntimeProgress?.campaignId === liveCampaign.campaignId ? (
                <div className="campaign-progress">
                  <div>
                    <span>{latestRuntimeProgress.phase}</span>
                    <strong>{latestRuntimeProgress.progress}%</strong>
                  </div>
                  <div className="progress-track">
                    <span style={{ width: `${latestRuntimeProgress.progress}%` }} />
                  </div>
                  <small>{latestRuntimeProgress.tokensPerSecond ? `${latestRuntimeProgress.tokensPerSecond.toFixed(1)} tokens/sec` : "waiting for generation"}</small>
                </div>
              ) : null}
              <div className="network-progress" aria-label="Network progress">
                <div className="network-progress-row">
                  <div>
                    <span>Network settlement</span>
                    <strong>{networkProgress.settlementPercent}%</strong>
                  </div>
                  <div className="progress-track">
                    <span style={{ width: `${networkProgress.settlementPercent}%` }} />
                  </div>
                  <small>{networkProgress.verifiedContributions}/{networkProgress.totalContributions} receipts verified</small>
                </div>
                <div className="network-progress-row">
                  <div>
                    <span>Work cleared</span>
                    <strong>{networkProgress.workPercent}%</strong>
                  </div>
                  <div className="progress-track">
                    <span style={{ width: `${networkProgress.workPercent}%` }} />
                  </div>
                  <small>{networkProgress.clearedWorkUnits}/{networkProgress.totalWorkUnits} work units moved</small>
                </div>
              </div>
            </article>
          ) : (
            <div className="empty-state compact">
              <ShieldCheck size={24} />
              <strong>{workModeEnabled ? "Guardian loop warming up" : "Verifier mode active"}</strong>
              <span>
                {workModeEnabled
                  ? "CYPHES will resolve the next indexed target, pin the commit, and create work if it has not been covered yet."
                  : "This node is syncing receipts and available for independent verification. Press Run to start local model work and campaign seeding."}
              </span>
            </div>
          )}

          {!liveCampaign && githubAccessStatus?.paused ? (
            <div className="cockpit-events intelligence-events standby-events" aria-label="GitHub pause status">
              <div className="cockpit-event danger pinned">
                <time>HOLD</time>
                <span>{githubPauseEventLabel(githubAccessStatus)}</span>
              </div>
            </div>
          ) : null}

          {!liveCampaign ? (
            <div className="runtime-radar standby-runtime-card">
              <label>
                <span>Provider</span>
                <select
                  onChange={(event) => setRuntimeProvider(event.currentTarget.value)}
                  value={runtimeProvider}
                >
                  <option value="lmstudio">LM Studio</option>
                  <option value="ollama">Ollama</option>
                </select>
              </label>
              <label>
                <span>Model</span>
                <select
                  disabled={runtimeModels.length === 0}
                  onChange={(event) => setRuntimeModel(event.currentTarget.value)}
                  value={runtimeModel}
                >
                  {runtimeModels.length === 0 ? (
                    <option value="">No model</option>
                  ) : (
                    runtimeModels.map((model) => (
                      <option key={model} value={model}>{model}</option>
                    ))
                  )}
                </select>
              </label>
              <button
                aria-label={workModeEnabled ? "Worker mode running" : "Run worker mode"}
                className="runtime-run-button"
                disabled={workModeEnabled || runtimeActive}
                onClick={() => {
                  void enableWorkMode();
                }}
                type="button"
              >
                <Play size={14} aria-hidden="true" />
                <span>{workModeEnabled ? "Running" : "Run"}</span>
              </button>
              <button
                aria-label="Stop worker mode"
                className="runtime-run-button runtime-stop-button"
                disabled={!workModeEnabled && !runtimeActive}
                onClick={stopWorkMode}
                type="button"
              >
                <Square size={13} aria-hidden="true" />
                <span>Stop</span>
              </button>
            </div>
          ) : null}

          <div className="watch-grid">
            <div>
              <Cpu size={14} />
              <small>Guardian Index</small>
              <strong>{guardianTargets.length} targets</strong>
              <span>Structured public DeFi coverage seed</span>
            </div>
            <div>
              <Target size={14} />
              <small>Next watch</small>
              <strong>{nextWatchTarget?.protocolName || "loading"}</strong>
              <span>{nextWatchTarget?.contractPaths[0] || "repository root"}</span>
            </div>
            <div>
              <ShieldCheck size={14} />
              <small>Last observed</small>
              <strong>{watchObservation?.lastObservedCommit ? shortCommit(watchObservation.lastObservedCommit) : "none"}</strong>
              <span>{watchObservation?.lastObservedAt ? new Date(watchObservation.lastObservedAt).toLocaleTimeString() : "waiting for first scan"}</span>
            </div>
          </div>
        </section>

        <div className="system-message-stack">
          {nodeError ? <div className="error-banner">Node error: {nodeError}</div> : null}
          {!isTauriRuntime() ? (
            <div className="preview-banner">
              Read-only browser preview. Signing, persistence, and networking require the native app.
            </div>
          ) : null}
        </div>

        <footer>
          <span>CYPHES v{APP_VERSION} testnet</span>
          <span>ATP envelope v0.3</span>
          <span>Ed25519 identity proof</span>
          <span>SQLite event chain</span>
          <span>Relay + direct libp2p</span>
          <span>ATP Credits are receipt-backed</span>
        </footer>
      </main>

      {notice ? <div className="notice">{notice}</div> : null}
    </div>
  );
}

export default function App() {
  return (
    <P2PProvider>
      <AppContent />
    </P2PProvider>
  );
}
