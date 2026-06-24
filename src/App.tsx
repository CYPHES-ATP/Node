import { listen } from "@tauri-apps/api/event";
import { type CSSProperties, FormEvent, useEffect, useMemo, useRef, useState } from "react";
import {
  ArrowRight,
  FileArchive,
  Gauge,
  Github,
  Link,
  LoaderCircle,
  ReceiptText,
  ShieldCheck,
  Trophy,
} from "lucide-react";
import { TitleBar } from "@/components/layout/TitleBar";
import { P2PProvider } from "@/components/providers/P2PProvider";
import { useP2P } from "@/hooks/useP2P";
import { isTauriRuntime, truncatePeerId } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";
import type {
  AuditJob,
  AuditRuntimeProgress,
  CampaignReportSnapshot,
  LocalModelList,
  ProtocolAuditCampaign,
  RepositorySummary,
} from "@/types";

const AUDIT_SCOPE = [
  "Dependency and supply-chain risk",
  "Secrets, permissions, and exposed configuration",
  "CI workflow and repository security posture",
  "Prioritized findings with reproducible evidence",
];

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
  tone?: "info" | "success" | "warn";
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

function deliveryLabel(job: AuditJob) {
  if (job.origin === "remote") return "Verified from peer";
  if (job.deliveryState === "acknowledged") {
    return `${job.acknowledgedPeers} peer ${job.acknowledgedPeers === 1 ? "receipt" : "receipts"}`;
  }
  return "Signed locally, no peer receipt";
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

function shortCommit(commitSha: string) {
  return commitSha ? commitSha.slice(0, 7).toUpperCase() : "UNPINNED";
}

function cockpitEventLabel(phase: string) {
  const normalized = phase.toLowerCase();
  if (normalized.includes("reading pinned github")) return "Fetched scoped files";
  if (normalized.includes("building model prompt")) return "Built audit prompt";
  if (normalized.includes("running local model")) return "Prompted local model";
  if (normalized.includes("parsing")) return "Parsed findings";
  if (normalized.includes("signing")) return "Signed contribution";
  if (normalized.includes("complete")) return "Audit skill complete";
  if (normalized.includes("preparing")) return "Loaded audit skill";
  return phase;
}

function formatClock(ms: number) {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

function AppContent() {
  const p2p = useP2P();
  const [repositoryUrl, setRepositoryUrl] = useState("");
  const [compensation, setCompensation] = useState("100");
  const [auditBrief, setAuditBrief] = useState("");
  const [attachmentText, setAttachmentText] = useState("");
  const [customSkillText, setCustomSkillText] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [actionJobId, setActionJobId] = useState<string | null>(null);
  const [peerAddress, setPeerAddress] = useState("");
  const [connecting, setConnecting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [runtimeProvider, setRuntimeProvider] = useState("lmstudio");
  const [runtimeModels, setRuntimeModels] = useState<string[]>([]);
  const [runtimeModel, setRuntimeModel] = useState("");
  const [runtimeStatus, setRuntimeStatus] = useState<LocalModelList | null>(null);
  const [runtimeProgress, setRuntimeProgress] = useState<Record<string, AuditRuntimeProgress>>({});
  const [latestRuntimeProgress, setLatestRuntimeProgress] = useState<AuditRuntimeProgress | null>(null);
  const [latestRuntimeEventAt, setLatestRuntimeEventAt] = useState(Date.now());
  const [runtimeStartedAt, setRuntimeStartedAt] = useState<number | null>(null);
  const [telemetryTick, setTelemetryTick] = useState(Date.now());
  const [runningWorkUnitId, setRunningWorkUnitId] = useState<string | null>(null);
  const [cockpitEvents, setCockpitEvents] = useState<CockpitEvent[]>([
    { id: "boot", label: "Runtime standby", at: Date.now() },
  ]);
  const [autoRunJobs, setAutoRunJobs] = useState<Record<string, true>>({});
  const runtimeStartedAtRef = useRef<number | null>(null);
  const lastPhaseRef = useRef("");

  const nodeStatus = useCyphesStore((state) => state.nodeStatus);
  const nodeError = useCyphesStore((state) => state.nodeError);
  const agentId = useCyphesStore((state) => state.agentId);
  const peerCount = useCyphesStore((state) => state.peerCount);
  const networkInfo = useCyphesStore((state) => state.networkInfo);
  const jobs = useCyphesStore((state) => state.jobs);
  const campaigns = useCyphesStore((state) => state.campaigns);
  const creditSummary = useCyphesStore((state) => state.creditSummary);
  const notice = useCyphesStore((state) => state.notice);
  const setNotice = useCyphesStore((state) => state.setNotice);
  const [campaignSnapshots, setCampaignSnapshots] = useState<Record<string, CampaignReportSnapshot>>({});

  const sortedJobs = useMemo(
    () => [...jobs].sort((a, b) => b.createdAt - a.createdAt),
    [jobs],
  );
  const relayAddress = networkInfo?.listen_addrs.find((address) =>
    address.includes("/p2p-circuit/"),
  );
  const runtimeProviderLabel = runtimeProvider === "ollama" ? "Ollama" : "LM Studio";
  const runtimeActive = Boolean(
    latestRuntimeProgress &&
    latestRuntimeProgress.progress > 0 &&
    latestRuntimeProgress.progress < 100,
  );
  const runtimeRecentlyFinished = Boolean(
    latestRuntimeProgress?.progress === 100 && telemetryTick - latestRuntimeEventAt < 8 * 60_000,
  );
  const elapsedRuntimeMs = runtimeStartedAt ? telemetryTick - runtimeStartedAt : 0;
  const progressDrift = runtimeActive
    ? Math.min(7, Math.floor((telemetryTick - latestRuntimeEventAt) / 850))
    : 0;
  const currentProgress = latestRuntimeProgress
    ? Math.min(100, latestRuntimeProgress.progress + progressDrift)
    : 0;
  const measuredTokensPerSecond = latestRuntimeProgress?.tokensPerSecond || 0;
  const samplingPulse = runtimeActive
    ? 0.7 + ((telemetryTick / 200) % 6) * 0.16
    : 0;
  const currentTokensPerSecond =
    measuredTokensPerSecond > 0
      ? measuredTokensPerSecond + (runtimeActive ? samplingPulse : 0)
      : runtimeActive
        ? samplingPulse
        : 0;
  const hasPendingCredit = runtimeActive || runtimeRecentlyFinished;
  const pendingReceiptMeter = hasPendingCredit ? Math.min(35, Math.max(1, Math.round(currentProgress * 0.35))) : 0;
  const runtimePhase =
    latestRuntimeProgress?.phase ||
    runtimeStatus?.message ||
    (runtimeModel ? "Runtime armed" : "Runtime offline");
  const cockpitStatus = runtimeActive
    ? "RUNNING AUDIT SKILL"
    : runtimeRecentlyFinished
      ? "SUBMITTED"
      : runtimeModel
        ? "ARMED"
        : "OFFLINE";
  const creditLabel = hasPendingCredit ? "Pending ATP" : "ATP earned";
  const creditValue = hasPendingCredit ? `+${pendingReceiptMeter}` : creditSummary.total.toString();

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

  async function refreshRuntimeModels(provider = runtimeProvider) {
    const listing = await p2p.listLocalModelModels(provider);
    setRuntimeStatus(listing);
    setRuntimeModels(listing.models);
    setRuntimeModel((current) => {
      if (current && listing.models.includes(current)) return current;
      return listing.models[0] || "";
    });
    return listing;
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
        pushCockpitEvent(cockpitEventLabel(event.payload.phase));
      }
    }).then((cleanup) => {
      cleanups.push(cleanup);
    });
    const eventListeners: Array<[string, string, CockpitEvent["tone"]]> = [
      ["audit:contribution_acknowledged", "Requester acknowledged receipt", "success"],
      ["audit:contribution_received", "Inbound contribution received", "success"],
      ["audit:verification_received", "Receipt verified; ATP earned", "success"],
      ["audit:verification_acknowledged", "Credit receipt delivered", "success"],
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
    const timer = window.setInterval(() => setTelemetryTick(Date.now()), 200);
    return () => window.clearInterval(timer);
  }, [runtimeActive, runtimeRecentlyFinished]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void refreshRuntimeModels(runtimeProvider);
    const timer = window.setInterval(() => {
      void refreshRuntimeModels(runtimeProvider);
    }, 15_000);
    return () => window.clearInterval(timer);
  }, [runtimeProvider]);

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
    if (!isTauriRuntime() || !agentId || !runtimeModel) return;
    const nextJob = jobs.find(
      (job) =>
        job.status === "routed" &&
        job.workerAgentId === agentId &&
        !job.resultHash &&
        !autoRunJobs[job.id],
    );
    if (!nextJob) return;
    setAutoRunJobs((current) => ({ ...current, [nextJob.id]: true }));
    void handleRunAcceptedAuditSkill(nextJob, true);
  }, [agentId, autoRunJobs, jobs, runtimeModel, runtimeProvider]);

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
      { ...repository, html_url: target.repoUrl },
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

  async function createAudit(event: FormEvent) {
    event.preventDefault();
    setFormError(null);

    const credits = Number(compensation);
    if (!Number.isFinite(credits) || credits <= 0) {
      setFormError("Enter an ATP Credits budget greater than zero.");
      return;
    }
    if (!isTauriRuntime()) {
      setFormError("Open the native CYPHES app to create a signed ATP request.");
      return;
    }
    if (nodeStatus !== "online" || !agentId) {
      setFormError("The local node is not ready yet.");
      return;
    }

    setSubmitting(true);
    try {
      const inspected = await inspectRepository(repositoryUrl);
      const { repository } = inspected;
      const job = await p2p.createAudit(
        repository,
        credits.toString(),
        inspected.scope,
        auditBrief,
        attachmentText,
        customSkillText,
      );
      setRepositoryUrl("");
      setAuditBrief("");
      setAttachmentText("");
      setCustomSkillText("");
      setNotice(
        peerCount > 0
          ? `Campaign and ATP request signed, committed, and sent to ${peerCount} ${peerCount === 1 ? "peer" : "peers"}.`
          : `Campaign and ATP request signed locally. Peer delivery is queued until discovery finds another node.`,
      );
      return job;
    } catch (error) {
      setFormError(error instanceof Error ? error.message : String(error));
    } finally {
      setSubmitting(false);
    }
  }

  async function handleOffer(job: AuditJob) {
    setActionJobId(job.id);
    try {
      await p2p.offerAudit(job.id);
      setNotice(`Signed worker offer sent for ${job.repository.fullName}.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  async function handleAcceptOffer(job: AuditJob) {
    setActionJobId(job.id);
    try {
      await p2p.acceptOffer(job.id);
      setNotice(`Worker selected for ${job.repository.fullName}.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  async function handleConnect(event: FormEvent) {
    event.preventDefault();
    if (!peerAddress.trim()) return;
    setConnecting(true);
    try {
      await p2p.connectPeer(peerAddress.trim());
      setNotice("Dialing the supplied libp2p address.");
      setPeerAddress("");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setConnecting(false);
    }
  }

  async function handleRoute(job: AuditJob) {
    setActionJobId(job.id);
    try {
      await p2p.routeAudit(job.id);
      setNotice("Requester-signed repository and artifact leases sent to the worker.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  async function handleRunAcceptedAuditSkill(job: AuditJob, automatic = false) {
    setActionJobId(job.id);
    try {
      if (!runtimeModel) {
        throw new Error(`Start ${runtimeProviderLabel}, load a local model, then select it.`);
      }
      pushCockpitEvent("Work order entered running state");
      const contributions = await p2p.runAcceptedAuditPipeline(
        job.id,
        runtimeProvider,
        runtimeModel,
      );
      const lastContribution = contributions[contributions.length - 1];
      pushCockpitEvent("Queued signed receipts for requester", "success");
      setNotice(
        `${automatic ? "Accepted work started automatically. " : ""}${runtimeModel} signed ${contributions.length} v0.4 audit passes${lastContribution ? ` through ${lastContribution.receiptHash.slice(0, 19)}...` : "."}`,
      );
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  async function refreshCampaignSnapshot(campaignId: string) {
    const snapshot = await p2p.getCampaignSnapshot(campaignId);
    setCampaignSnapshots((current) => ({ ...current, [campaignId]: snapshot }));
    return snapshot;
  }

  async function handleRunAuditPipeline(campaign: ProtocolAuditCampaign) {
    setActionJobId(campaign.campaignId);
    try {
      if (!runtimeModel) {
        throw new Error(`Start ${runtimeProviderLabel}, load a local model, then select it.`);
      }
      pushCockpitEvent("Pipeline entered running state");
      const contributions = await p2p.runCampaignAuditPipeline(
        campaign.campaignId,
        runtimeProvider,
        runtimeModel,
      );
      await refreshCampaignSnapshot(campaign.campaignId);
      pushCockpitEvent("Signed pipeline contribution bundle", "success");
      setNotice(
        `${runtimeModel} produced ${contributions.length} signed v0.4 audit passes.`,
      );
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  async function handleClaimWorkUnit(campaign: ProtocolAuditCampaign, workUnitId?: string) {
    const actionId = workUnitId ? `${campaign.campaignId}:${workUnitId}` : campaign.campaignId;
    setActionJobId(actionId);
    try {
      const snapshot =
        campaignSnapshots[campaign.campaignId] ||
        (await refreshCampaignSnapshot(campaign.campaignId));
      const unit = workUnitId
        ? snapshot.workUnits.find((item) => item.workUnitId === workUnitId)
        : snapshot.workUnits.find((item) => item.status === "open");
      if (!unit) throw new Error("No open work unit is claimable.");
      if (unit.status !== "open") throw new Error(`${unit.title} is not open for claim.`);
      const claim = await p2p.claimCampaignWorkUnit(campaign.campaignId, unit.workUnitId);
      await refreshCampaignSnapshot(campaign.campaignId);
      pushCockpitEvent(`${unit.title} claimed`);
      setNotice(`Claimed ${unit.title}. Claim ${claim.claimId.slice(0, 22)}... sent to requester.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  async function handleRunClaimedWorkUnit(campaign: ProtocolAuditCampaign, workUnitId?: string) {
    const actionId = workUnitId ? `${campaign.campaignId}:${workUnitId}` : campaign.campaignId;
    setActionJobId(actionId);
    try {
      if (!runtimeModel) {
        throw new Error(`Start ${runtimeProviderLabel}, load a local model, then select it.`);
      }
      const snapshot =
        campaignSnapshots[campaign.campaignId] ||
        (await refreshCampaignSnapshot(campaign.campaignId));
      const claim = snapshot.claims.find(
        (item) =>
          item.workerAgentId === agentId &&
          item.status === "claimed" &&
          (!workUnitId || item.workUnitId === workUnitId),
      );
      if (!claim) throw new Error("Claim a work unit before running it.");
      setRunningWorkUnitId(claim.workUnitId);
      pushCockpitEvent("Claimed unit moved to running");
      const contribution = await p2p.runClaimedWorkUnit(
        campaign.campaignId,
        claim.workUnitId,
        runtimeProvider,
        runtimeModel,
      );
      await refreshCampaignSnapshot(campaign.campaignId);
      pushCockpitEvent("Signed contribution stored locally", "success");
      pushCockpitEvent("Broadcasted receipt to requester", "success");
      setNotice(`${runtimeModel} produced signed work for ${campaign.protocolName}; receipt ${contribution.receiptHash.slice(0, 19)}... sent.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setRunningWorkUnitId(null);
      setActionJobId(null);
    }
  }

  async function handleVerifyQueue(campaign: ProtocolAuditCampaign) {
    setActionJobId(campaign.campaignId);
    try {
      const snapshot = await refreshCampaignSnapshot(campaign.campaignId);
      const verifiedIds = new Set(snapshot.verifications.map((item) => item.targetContributionId));
      const pending = snapshot.contributions.filter(
        (item) => !verifiedIds.has(item.contributionId),
      );
      if (pending.length === 0) throw new Error("No unverified contribution is available.");
      const issued = [];
      for (const contribution of pending) {
        issued.push(...(await p2p.verifyCampaignContribution(contribution.contributionId)));
      }
      await refreshCampaignSnapshot(campaign.campaignId);
      const total = issued.reduce((sum, item) => sum + item.total, 0);
      pushCockpitEvent(`Receipt verified; ATP earned +${total}`, "success");
      setNotice(`${pending.length} contribution${pending.length === 1 ? "" : "s"} accepted; ${total} ATP Credits issued from signed receipts.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  async function handleExportCampaign(campaign: ProtocolAuditCampaign) {
    setActionJobId(campaign.campaignId);
    try {
      const bundle = await p2p.exportCampaignReport(campaign.campaignId);
      setNotice(`Final audit report bundle exported to ${bundle.bundlePath}.`);
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  async function handleApprove(job: AuditJob) {
    setActionJobId(job.id);
    try {
      await p2p.approveResult(job.id);
      setNotice("Verified result approved; settlement sent for worker attestation.");
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  function jobAction(job: AuditJob, isMine: boolean) {
    if (!job.repository.commitSha) {
      return <div className="job-outcome">Legacy unpinned request; repost required</div>;
    }
    if (job.status === "discovered" && !isMine) {
      return (
        <button
          disabled={actionJobId === job.id}
          onClick={() => void handleOffer(job)}
          type="button"
        >
          {actionJobId === job.id ? "Signing offer" : "Offer to audit"}
          <ArrowRight size={14} />
        </button>
      );
    }
    if (job.status === "negotiating" && isMine) {
      return (
        <button
          disabled={actionJobId === job.id}
          onClick={() => void handleAcceptOffer(job)}
          type="button"
        >
          {actionJobId === job.id ? "Signing selection" : "Select worker"}
          <ArrowRight size={14} />
        </button>
      );
    }
    if (job.status === "negotiated" && isMine) {
      return (
        <button
          disabled={actionJobId === job.id}
          onClick={() => void handleRoute(job)}
          type="button"
        >
          {actionJobId === job.id ? "Signing lease" : "Issue context lease"}
          <ArrowRight size={14} />
        </button>
      );
    }
    if (job.status === "routed" && !isMine && !job.resultHash) {
      return (
        <button
          disabled={actionJobId === job.id || !runtimeModel}
          onClick={() => void handleRunAcceptedAuditSkill(job)}
          type="button"
        >
          {actionJobId === job.id ? "Running pipeline" : runtimeModel ? "Run audit pipeline" : "Select local model"}
          <ArrowRight size={14} />
        </button>
      );
    }
    if (job.status === "routed" && isMine && job.resultHash) {
      return (
        <button
          disabled={actionJobId === job.id}
          onClick={() => void handleApprove(job)}
          type="button"
        >
          {actionJobId === job.id ? "Signing approval" : "Approve verified result"}
          <ArrowRight size={14} />
        </button>
      );
    }

    let outcome = deliveryLabel(job);
    if (job.status === "negotiating") {
      outcome = isMine
        ? `Offer from ${truncatePeerId(job.workerAgentId || "")}`
        : "Offer committed, awaiting requester";
    } else if (job.status === "negotiated") {
      outcome = isMine
        ? `Worker selected: ${truncatePeerId(job.workerAgentId || "")}`
        : "Selected; awaiting requester lease";
    } else if (job.status === "routed") {
      outcome = job.resultHash
        ? isMine
          ? "Signed result verified"
          : "Result sent; awaiting approval"
        : isMine
          ? "Lease active; awaiting worker result"
          : "Requester lease verified";
    } else if (job.status === "settled") {
      outcome = "Requester approved; awaiting worker receipt";
    } else if (job.status === "attested") {
      outcome = job.bundlePath
        ? `Receipt: ${job.bundlePath}`
        : `Proof of Cognition: ${job.receiptHash?.slice(0, 19) || "committed"}`;
    }
    return <div className="job-outcome">{outcome}</div>;
  }

  const showLegacyProtocolMechanics = false;

  return (
    <div className="app-shell">
      <TitleBar />

      <main>
        <section className="runtime-terminal" aria-label="Runtime terminal">
          <div className="terminal-controls">
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
          </div>

          <div className="cockpit-display">
            <div className="terminal-status">
              <div>
                <span>{cockpitStatus}</span>
                <strong>{runtimePhase}</strong>
              </div>
              <code>{runtimeActive ? "LIVE" : runtimeRecentlyFinished ? "AWAITING VERIFIER" : runtimeModel ? "READY" : "NO MODEL"}</code>
            </div>

            <div className="terminal-metrics">
              <div className="metric-card metric-card-wide">
                <Gauge size={17} />
                <span>Tokens/sec</span>
                <strong>{currentTokensPerSecond.toFixed(1)}</strong>
                <small>{runtimeActive ? "200ms live cockpit sample" : measuredTokensPerSecond ? "last measured run" : "waiting for model"}</small>
              </div>
              <div className="metric-card">
                <Trophy size={17} />
                <span>{creditLabel}</span>
                <strong>{creditValue}</strong>
                <small>{hasPendingCredit ? "provisional only" : "receipt-backed"}</small>
              </div>
              <div className="metric-card">
                <span>Progress</span>
                <strong>{currentProgress ? `${currentProgress}%` : runtimeModel ? "armed" : "offline"}</strong>
                <small>{runtimeActive ? formatClock(elapsedRuntimeMs) : "runtime clock"}</small>
              </div>
              <div className="metric-card">
                <span>Peers</span>
                <strong>{peerCount}</strong>
                <small>{networkInfo?.relay_connected ? "relay linked" : "network standby"}</small>
              </div>
            </div>

            <div className="terminal-progress" aria-label="Audit skill progress">
              <span style={{ width: `${currentProgress}%` } as CSSProperties} />
            </div>

            <div className="cockpit-events" aria-label="Live runtime event stream">
              {cockpitEvents.map((event) => (
                <div className={`cockpit-event ${event.tone || "info"}`} key={event.id}>
                  <time>{formatClock(telemetryTick - event.at)}</time>
                  <span>{event.label}</span>
                </div>
              ))}
            </div>
          </div>
        </section>

        <details className="manual-connect" open={!networkInfo?.relay_configured}>
          <summary>Manual peer connection</summary>
          <form className="connect-strip" onSubmit={(event) => void handleConnect(event)}>
            <Link size={15} />
            <label htmlFor="peer-address">Peer multiaddress</label>
            <input
              id="peer-address"
              onChange={(event) => setPeerAddress(event.currentTarget.value)}
              placeholder="Optional multiaddress fallback"
              spellCheck={false}
              value={peerAddress}
            />
            <button disabled={connecting || !peerAddress.trim()} type="submit">
              {connecting ? "Dialing" : "Connect"}
            </button>
          </form>
          {relayAddress ? (
            <div className="share-address">
              <span>Your relay address</span>
              <code>{relayAddress}</code>
            </div>
          ) : null}
        </details>

        {nodeError ? <div className="error-banner">Node error: {nodeError}</div> : null}
        {!isTauriRuntime() ? (
          <div className="preview-banner">
            Read-only browser preview. Signing, persistence, and networking require the native app.
          </div>
        ) : null}

        {showLegacyProtocolMechanics ? (
        <div className="workspace">
          <section className="panel compose-panel">
            <div className="section-heading">
              <span>01</span>
              <div>
                <h2>Create protocol audit campaign</h2>
                <p>Public GitHub repository, pinned commit, signed ATP request.</p>
              </div>
            </div>

            <form onSubmit={(event) => void createAudit(event)}>
              <label htmlFor="repository-url">Repository URL</label>
              <div className="input-shell">
                <Github size={18} />
                <input
                  id="repository-url"
                  onChange={(event) => setRepositoryUrl(event.currentTarget.value)}
                  placeholder="https://github.com/owner/repository"
                  spellCheck={false}
                  type="url"
                  value={repositoryUrl}
                />
              </div>

              <label htmlFor="compensation">PAY with ATP</label>
              <div className="compensation-row">
                <div className="input-shell">
                  <input
                    id="compensation"
                    min="1"
                    onChange={(event) => setCompensation(event.currentTarget.value)}
                    step="1"
                    type="number"
                    value={compensation}
                  />
                  <span>ATP Credits</span>
                </div>
                <p>Receipt-backed accounting only. No ERC-20, escrow, or bounty payout is represented in this build.</p>
              </div>

              <label htmlFor="audit-brief">Audit brief</label>
              <div className="input-shell textarea-shell">
                <textarea
                  id="audit-brief"
                  onChange={(event) => setAuditBrief(event.currentTarget.value)}
                  placeholder="Requester guidance, scope notes, bounty rules, threat model, or concerns."
                  spellCheck={false}
                  value={auditBrief}
                />
              </div>

              <label htmlFor="attachments">Attachments / protocol docs</label>
              <div className="input-shell textarea-shell">
                <textarea
                  id="attachments"
                  onChange={(event) => setAttachmentText(event.currentTarget.value)}
                  placeholder="Paste bounty policy, protocol docs, or PDF excerpts. CYPHES hashes this text into the campaign."
                  spellCheck={false}
                  value={attachmentText}
                />
              </div>

              <details className="advanced-skill">
                <summary>Advanced: custom SKILL.md overlay</summary>
                <div className="input-shell textarea-shell">
                  <textarea
                    onChange={(event) => setCustomSkillText(event.currentTarget.value)}
                    placeholder="Paste an optional SKILL.md overlay. CYPHES keeps the base skill pack and records this hash in receipts."
                    spellCheck={false}
                    value={customSkillText}
                  />
                </div>
              </details>

              {formError ? <div className="form-error">{formError}</div> : null}

              <button
                className="primary-action"
                disabled={
                  submitting ||
                  !repositoryUrl.trim() ||
                  nodeStatus !== "online" ||
                  !isTauriRuntime()
                }
                type="submit"
              >
                {submitting ? <LoaderCircle className="spin" size={16} /> : <ShieldCheck size={16} />}
                {submitting ? "Checking repository" : "Sign campaign"}
                {!submitting ? <ArrowRight size={16} /> : null}
              </button>
            </form>
          </section>

          <section className="panel jobs-panel">
            <div className="section-heading">
              <span>02</span>
              <div>
                <h2>ATP transactions</h2>
              </div>
            </div>

            <div className="jobs-list">
              {sortedJobs.length === 0 ? (
                <div className="empty-state">
                  <Github size={24} />
                  <strong>No committed audit requests</strong>
                  <span>Post a repository or wait for signed work from another CYPHES node.</span>
                </div>
              ) : (
                sortedJobs.map((job) => {
                  const isMine = job.requesterAgentId === agentId;
                  return (
                    <article className="job-card" key={job.id}>
                      <div className="job-topline">
                        <span className={`job-status ${job.status}`}>{job.status}</span>
                        <span>{isMine ? "Requested by you" : `From ${truncatePeerId(job.requesterAgentId)}`}</span>
                      </div>
                      <h3>{job.repository.fullName}</h3>
                      {job.repository.description ? <p>{job.repository.description}</p> : null}
                      <div className="repo-meta">
                        <span>{job.repository.language || "Language unknown"}</span>
                        <span>
                          {job.repository.defaultBranch}
                          {job.repository.commitSha
                            ? `@${job.repository.commitSha.slice(0, 7)}`
                            : ""}
                        </span>
                        <span>{job.repository.stars.toLocaleString()} stars</span>
                        {job.receiptHash ? (
                          <span className="receipt-chip">
                            <ReceiptText size={11} />
                            Proof of Cognition
                          </span>
                        ) : null}
                      </div>
                      <div className="job-footer">
                        <div>
                          <span>Proposed</span>
                          <strong>{job.compensation} {job.currency}</strong>
                        </div>
                        {jobAction(job, isMine)}
                      </div>
                    </article>
                  );
                })
              )}
            </div>
          </section>
        </div>
        ) : null}

        <section className="panel labor-panel">
          <div className="section-heading">
            <span>Live</span>
            <div>
              <h2>Work Orders</h2>
            </div>
          </div>

          <div className="campaign-list">
            {campaigns.length === 0 ? (
              <div className="empty-state compact">
                <ShieldCheck size={24} />
                <strong>No protocol campaigns yet</strong>
                <span>Create a campaign from campaign.html; this app receives signed work orders.</span>
              </div>
            ) : (
              campaigns.map((campaign) => {
                const snapshot = campaignSnapshots[campaign.campaignId];
                const progress = runtimeProgress[campaign.campaignId];
                const workUnits = snapshot?.workUnits || [];
                const contributions = snapshot?.contributions.length || 0;
                const accepted = snapshot?.verifications.filter((item) => item.decision === "accepted").length || 0;
                const unverified = contributions - (snapshot?.verifications.length || 0);
                const isMine = campaign.requesterAgentId === agentId;
                const claimedCount = snapshot?.claims.filter((item) => item.status === "claimed").length || 0;
                const contributionWorkUnitById = new Map(
                  (snapshot?.contributions || []).map((item) => [item.contributionId, item.workUnitId]),
                );
                const latestVerificationDecisionByWorkUnit = new Map<string, string>();
                (snapshot?.verifications || []).forEach((item) => {
                  const workUnitId = contributionWorkUnitById.get(item.targetContributionId);
                  if (workUnitId) {
                    latestVerificationDecisionByWorkUnit.set(workUnitId, item.decision);
                  }
                });
                const isUnitCompleted = (workUnitId: string, status: string) =>
                  status === "accepted" ||
                  latestVerificationDecisionByWorkUnit.get(workUnitId) === "accepted";
                const openUnitCount = workUnits.filter((unit) => unit.status === "open").length;
                const completedUnitCount = workUnits.filter((unit) =>
                  isUnitCompleted(unit.workUnitId, unit.status),
                ).length;
                const activeUnitCount = workUnits.filter(
                  (unit) => unit.status !== "open" && !isUnitCompleted(unit.workUnitId, unit.status),
                ).length;
                const workUnitSummary = [
                  `${openUnitCount} open`,
                  activeUnitCount > 0 ? `${activeUnitCount} active` : null,
                  completedUnitCount > 0 ? `${completedUnitCount} completed` : null,
                ].filter(Boolean).join(" / ");
                return (
                  <article className="campaign-card" key={campaign.campaignId}>
                    <div className="campaign-main">
                      <div className="job-topline">
                        <span className="job-status routed">{campaign.status}</span>
                        <span>{campaign.repository.fullName}</span>
                      </div>
                      <h3>{campaign.protocolName}</h3>
                      <div className="campaign-target">
                        <span>{campaignFocus(campaign)}</span>
                        <code>{shortCommit(campaign.repository.commitSha)}</code>
                      </div>
                      <div className="repo-meta">
                        <span>{workUnits.length} work units</span>
                        <span>{claimedCount} claimed</span>
                        <span>{contributions} contributions</span>
                        <span>{accepted} accepted</span>
                        <span>
                          {contributions === 0
                            ? "no local-model audit yet"
                            : unverified > 0
                              ? `${unverified} unverified`
                              : "verified queue clear"}
                        </span>
                      </div>
                      {progress ? (
                        <div className="campaign-progress">
                          <div>
                            <span>{progress.phase}</span>
                            <strong>{progress.progress}%</strong>
                          </div>
                          <div className="progress-track">
                            <span style={{ width: `${progress.progress}%` }} />
                          </div>
                          <small>{progress.tokensPerSecond ? `${progress.tokensPerSecond.toFixed(1)} tokens/sec` : "waiting for generation"}</small>
                        </div>
                      ) : null}
                    </div>
                    <div className="campaign-actions">
                      {isMine ? (
                        <>
                          <button
                            disabled={actionJobId === campaign.campaignId || !runtimeModel}
                            onClick={() => void handleRunAuditPipeline(campaign)}
                            type="button"
                          >
                            Run local pipeline
                            <ArrowRight size={14} />
                          </button>
                          <button
                            disabled={actionJobId === campaign.campaignId}
                            onClick={() => void handleVerifyQueue(campaign)}
                            type="button"
                          >
                            {unverified > 0 ? `Verify ${unverified}` : "Verify queue"}
                            <ShieldCheck size={14} />
                          </button>
                          <button
                            disabled={actionJobId === campaign.campaignId || contributions === 0}
                            onClick={() => void handleExportCampaign(campaign)}
                            type="button"
                            title={
                              contributions === 0
                                ? "Run or receive signed work before exporting a report bundle."
                                : undefined
                            }
                          >
                            Export report
                            <FileArchive size={14} />
                          </button>
                        </>
                      ) : (
                        <div className="job-outcome">Claim and run individual work units.</div>
                      )}
                    </div>
                    <details className="work-unit-dropdown">
                        <summary>
                          <span>Select work unit</span>
                          <strong>
                            {isMine && unverified > 0
                              ? `${unverified} needs verification`
                              : workUnitSummary}
                          </strong>
                        </summary>
                        <div className="work-unit-list">
                        {workUnits.map((unit) => {
                          const unitContributions = snapshot?.contributions.filter(
                            (item) => item.workUnitId === unit.workUnitId,
                          ) || [];
                          const contributionIds = new Set(
                            unitContributions.map((item) => item.contributionId),
                          );
                          const unitVerifications = snapshot?.verifications.filter((item) =>
                            contributionIds.has(item.targetContributionId),
                          ) || [];
                          const myClaim = snapshot?.claims.find(
                            (item) =>
                              item.workUnitId === unit.workUnitId &&
                              item.workerAgentId === agentId &&
                              item.status === "claimed",
                          );
                          const myContribution = unitContributions.find(
                            (item) => item.workerAgentId === agentId,
                          );
                          const latestVerification = unitVerifications[unitVerifications.length - 1];
                          const actionId = `${campaign.campaignId}:${unit.workUnitId}`;
                          const isRunningUnit = runningWorkUnitId === unit.workUnitId || actionJobId === actionId;
                          const unitStatusLabel = isRunningUnit
                            ? "running"
                            : isUnitCompleted(unit.workUnitId, unit.status)
                            ? "completed"
                            : unit.status;
                          const verifierState = latestVerification?.decision
                            ? latestVerification.decision === "accepted"
                              ? "completed"
                              : latestVerification.decision
                            : unitContributions.length > 0
                              ? "awaiting verifier"
                              : unit.status;
                          const claimedBy = unit.claimedByAgentId
                            ? truncatePeerId(unit.claimedByAgentId)
                            : "open";
                          return (
                            <div className="work-unit-row" key={unit.workUnitId}>
                              <div className="work-unit-main">
                                <strong>{unit.title}</strong>
                                <span>{unit.kind}</span>
                              </div>
                              <div className="work-unit-cell">
                                <small>Status</small>
                                <span className={`unit-pill ${unitStatusLabel.replace(/\s+/g, "_")}`}>{unitStatusLabel}</span>
                              </div>
                              <div className="work-unit-cell">
                                <small>Claimed by</small>
                                <span>{claimedBy}</span>
                              </div>
                              <div className="work-unit-cell">
                                <small>Contrib</small>
                                <span>{unitContributions.length}</span>
                              </div>
                              <div className="work-unit-cell">
                                <small>Verifier</small>
                                <span>{verifierState}</span>
                              </div>
                              <div className="work-unit-action">
                                {isMine ? unitContributions.length > unitVerifications.length ? (
                                  <button
                                    className="needs-action-button"
                                    disabled={actionJobId === campaign.campaignId}
                                    onClick={() => void handleVerifyQueue(campaign)}
                                    type="button"
                                  >
                                    Verify
                                  </button>
                                ) : (
                                  <span>{unitContributions.length > 0 ? "completed" : "awaiting node"}</span>
                                ) : unit.status === "open" ? (
                                  <button
                                    disabled={actionJobId === actionId}
                                    onClick={() => void handleClaimWorkUnit(campaign, unit.workUnitId)}
                                    type="button"
                                  >
                                    Claim
                                  </button>
                                ) : myContribution ? (
                                  <span>
                                    {latestVerification
                                      ? verifierState === "completed"
                                        ? "Completed · receipt verified"
                                        : verifierState
                                      : "Submitted · awaiting requester"}
                                  </span>
                                ) : myClaim ? (
                                  <button
                                    disabled={actionJobId === actionId || !runtimeModel}
                                    onClick={() => void handleRunClaimedWorkUnit(campaign, unit.workUnitId)}
                                    type="button"
                                  >
                                    {isRunningUnit ? "Running" : runtimeModel ? "Run" : "Model"}
                                  </button>
                                ) : (
                                  <span>{unit.claimedByAgentId ? "claimed" : "locked"}</span>
                                )}
                              </div>
                            </div>
                          );
                        })}
                        </div>
                      </details>
                  </article>
                );
              })
            )}
          </div>
        </section>

        <footer>
          <span>ATP v0.3 envelopes</span>
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
