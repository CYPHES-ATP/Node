import { listen } from "@tauri-apps/api/event";
import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  ArrowRight,
  Check,
  Cpu,
  Database,
  FileArchive,
  Gauge,
  Github,
  Link,
  LoaderCircle,
  RadioTower,
  ReceiptText,
  ShieldCheck,
  Trophy,
  Users,
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

function AppContent() {
  const p2p = useP2P();
  const [repositoryUrl, setRepositoryUrl] = useState("");
  const [compensation, setCompensation] = useState("100");
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
    let unlisten: (() => void) | null = null;
    listen<AuditRuntimeProgress>("audit:runtime_progress", (event) => {
      if (disposed) return;
      setLatestRuntimeProgress(event.payload);
      setRuntimeProgress((current) => ({
        ...current,
        [event.payload.campaignId]: event.payload,
      }));
    }).then((cleanup) => {
      unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

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
      );
      await p2p.createProtocolCampaign(
        repository,
        repository.fullName.split("/")[0],
        inspected.scopeText,
        credits.toString(),
      );
      setRepositoryUrl("");
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

  async function handleRun(job: AuditJob) {
    setActionJobId(job.id);
    try {
      await p2p.runAudit(job.id);
      setNotice("ATP repository worker completed; signed result sent to the requester.");
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

  async function handleRunAuditSkill(campaign: ProtocolAuditCampaign) {
    setActionJobId(campaign.campaignId);
    try {
      if (!runtimeModel) {
        throw new Error(`Start ${runtimeProviderLabel}, load a local model, then select it.`);
      }
      const snapshot = await refreshCampaignSnapshot(campaign.campaignId);
      const unit =
        snapshot.workUnits.find((item) => item.status === "open") ||
        snapshot.workUnits[0];
      if (!unit) throw new Error("Campaign has no work units.");
      const contribution = await p2p.runCampaignAuditSkill(
        campaign.campaignId,
        unit.workUnitId,
        runtimeProvider,
        runtimeModel,
      );
      await refreshCampaignSnapshot(campaign.campaignId);
      setNotice(
        `${contribution.runtime?.model || runtimeModel} produced signed contribution ${contribution.receiptHash.slice(0, 19)}...`,
      );
    } catch (error) {
      setNotice(error instanceof Error ? error.message : String(error));
    } finally {
      setActionJobId(null);
    }
  }

  async function handleVerifyLatest(campaign: ProtocolAuditCampaign) {
    setActionJobId(campaign.campaignId);
    try {
      const snapshot = await refreshCampaignSnapshot(campaign.campaignId);
      const verifiedIds = new Set(snapshot.verifications.map((item) => item.targetContributionId));
      const contribution = snapshot.contributions.find(
        (item) => !verifiedIds.has(item.contributionId),
      );
      if (!contribution) throw new Error("No unverified contribution is available.");
      const credits = await p2p.verifyCampaignContribution(contribution.contributionId);
      await refreshCampaignSnapshot(campaign.campaignId);
      const total = credits.reduce((sum, item) => sum + item.total, 0);
      setNotice(`Contribution accepted; ${total} ATP Credits issued from signed receipts.`);
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
          disabled={actionJobId === job.id}
          onClick={() => void handleRun(job)}
          type="button"
        >
          {actionJobId === job.id ? "Running worker" : "Run ATP worker"}
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

  return (
    <div className="app-shell">
      <TitleBar />

      <main>
        <section aria-label="Current capabilities" className="truth-strip">
          <div>
            <Database size={15} />
            <span>ATP state</span>
            <strong>Signed + SQLite</strong>
          </div>
          <div>
            <Users size={15} />
            <span>Connected</span>
            <strong>{peerCount} {peerCount === 1 ? "peer" : "peers"}</strong>
          </div>
          <div>
            <RadioTower size={15} />
            <span>Internet network</span>
            <strong className={networkInfo?.rendezvous_registered ? "" : "warning"}>
              {networkInfo?.rendezvous_registered
                ? "Discoverable"
                : networkInfo?.relay_connected
                  ? "Registering"
                  : networkInfo?.relay_configured
                    ? "Connecting"
                    : "Not configured"}
            </strong>
          </div>
          <div>
            <ShieldCheck size={15} />
            <span>Audit runtime</span>
            <strong className={runtimeModel ? "" : "warning"}>
              {runtimeModel || "No local model"}
            </strong>
          </div>
          <div>
            <Trophy size={15} />
            <span>ATP earned</span>
            <strong>{creditSummary.total} credits</strong>
          </div>
        </section>

        <section className="runtime-panel" aria-label="Audit runtime">
          <div className="runtime-copy">
            <Cpu size={16} />
            <div>
              <span>Audit Runtime</span>
              <strong>Local models only</strong>
              <p>No API key. CYPHES uses the local model server already running on this Mac.</p>
            </div>
          </div>
          <label>
            Provider
            <select
              onChange={(event) => setRuntimeProvider(event.currentTarget.value)}
              value={runtimeProvider}
            >
              <option value="lmstudio">LM Studio</option>
              <option value="ollama">Ollama</option>
            </select>
          </label>
          <label>
            Model
            <select
              disabled={runtimeModels.length === 0}
              onChange={(event) => setRuntimeModel(event.currentTarget.value)}
              value={runtimeModel}
            >
              {runtimeModels.length === 0 ? (
                <option value="">No models detected</option>
              ) : (
                runtimeModels.map((model) => (
                  <option key={model} value={model}>{model}</option>
                ))
              )}
            </select>
          </label>
          <div className="runtime-meter">
            <div>
              <Gauge size={14} />
              <span>{latestRuntimeProgress?.phase || runtimeStatus?.message || "Waiting for local model"}</span>
              <strong>{latestRuntimeProgress ? `${latestRuntimeProgress.progress}%` : runtimeModel ? "Ready" : "Offline"}</strong>
            </div>
            <div className="progress-track">
              <span style={{ width: `${latestRuntimeProgress?.progress || 0}%` }} />
            </div>
          </div>
          <div className="token-gauge">
            <span>Tokens/sec</span>
            <strong>
              {latestRuntimeProgress?.tokensPerSecond
                ? latestRuntimeProgress.tokensPerSecond.toFixed(1)
                : "0.0"}
            </strong>
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
        </details>

        {relayAddress ? (
          <div className="share-address">
            <span>Your relay address</span>
            <code>{relayAddress}</code>
          </div>
        ) : null}

        {nodeError ? <div className="error-banner">Node error: {nodeError}</div> : null}
        {!isTauriRuntime() ? (
          <div className="preview-banner">
            Read-only browser preview. Signing, persistence, and networking require the native app.
          </div>
        ) : null}

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

              <div className="scope">
                <span className="scope-label">Audit scope</span>
                {AUDIT_SCOPE.map((item) => (
                  <div key={item}>
                    <Check size={14} />
                    <span>{item}</span>
                  </div>
                ))}
              </div>

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
                <p>Existing repository-audit flow stays intact.</p>
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

        <section className="panel labor-panel">
          <div className="section-heading">
            <span>03</span>
            <div>
              <h2>Audit labor network</h2>
              <p>Campaigns, work units, signed contributions, verifier decisions, and ATP Credits.</p>
            </div>
          </div>

          <div className="campaign-list">
            {campaigns.length === 0 ? (
              <div className="empty-state compact">
                <ShieldCheck size={24} />
                <strong>No protocol campaigns yet</strong>
                <span>Create a campaign above to decompose it into verifiable work units.</span>
              </div>
            ) : (
              campaigns.map((campaign) => {
                const snapshot = campaignSnapshots[campaign.campaignId];
                const progress = runtimeProgress[campaign.campaignId];
                const accepted = snapshot?.verifications.filter((item) => item.decision === "accepted").length || 0;
                const unverified = (snapshot?.contributions.length || 0) - (snapshot?.verifications.length || 0);
                return (
                  <article className="campaign-card" key={campaign.campaignId}>
                    <div>
                      <div className="job-topline">
                        <span className="job-status routed">{campaign.status}</span>
                        <span>{campaign.repository.fullName}@{campaign.repository.commitSha.slice(0, 7)}</span>
                      </div>
                      <h3>{campaign.protocolName}</h3>
                      <p>{campaign.scopeText}</p>
                      <div className="repo-meta">
                        <span>{snapshot?.workUnits.length || 0} work units</span>
                        <span>{snapshot?.contributions.length || 0} contributions</span>
                        <span>{accepted} accepted</span>
                        <span>{unverified > 0 ? `${unverified} unverified` : "verified queue clear"}</span>
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
                      <button
                        disabled={actionJobId === campaign.campaignId || !runtimeModel}
                        onClick={() => void handleRunAuditSkill(campaign)}
                        type="button"
                      >
                        Run Audit Skill
                        <ArrowRight size={14} />
                      </button>
                      <button
                        disabled={actionJobId === campaign.campaignId}
                        onClick={() => void handleVerifyLatest(campaign)}
                        type="button"
                      >
                        Verify latest
                        <ShieldCheck size={14} />
                      </button>
                      <button
                        disabled={actionJobId === campaign.campaignId}
                        onClick={() => void handleExportCampaign(campaign)}
                        type="button"
                      >
                        Export report
                        <FileArchive size={14} />
                      </button>
                    </div>
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
