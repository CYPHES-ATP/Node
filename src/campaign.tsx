import { invoke } from "@tauri-apps/api/core";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import React, { FormEvent, useEffect, useMemo, useState } from "react";
import ReactDOM from "react-dom/client";
import { isTauriRuntime, truncatePeerId } from "@/lib/utils";
import type {
  AuditJob,
  BackendPeerInfo,
  CampaignReportSnapshot,
  CreditAllocation,
  CreditSummary,
  ExportedReportBundle,
  NetworkInfo,
  ProtocolAuditCampaign,
  RepositorySummary,
} from "@/types";
import "./styles/globals.css";

const AUDIT_SCOPE = [
  "Dependency and supply-chain risk",
  "Secrets, permissions, and exposed configuration",
  "CI workflow and repository security posture",
  "Prioritized findings with reproducible evidence",
];

interface StartNodeResponse {
  peer_id: string;
  agent_id: string;
}

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

interface AdminState {
  campaigns: ProtocolAuditCampaign[];
  jobs: AuditJob[];
  peers: BackendPeerInfo[];
  credits: CreditSummary;
  networkInfo: NetworkInfo | null;
  snapshots: Record<string, CampaignReportSnapshot>;
}

interface LatestExport {
  campaignId: string;
  bundlePath: string;
  reportPath: string;
}

function parseGitHubInput(value: string): GitHubInputTarget | null {
  let parsed: URL;
  try {
    parsed = new URL(value.trim());
  } catch {
    return null;
  }
  if (parsed.protocol !== "https:" || !/^(www\.)?github\.com$/i.test(parsed.hostname)) {
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
  const route = segments[2]?.toLowerCase();
  const kind = route === "blob" || route === "tree" ? route : "repository";
  return {
    apiUrl: `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo)}`,
    repoUrl: `https://github.com/${owner}/${repo}`,
    kind,
    pathSegments: kind === "repository" ? [] : segments.slice(3),
  };
}

async function resolveCommit(apiUrl: string, ref: string, optional = false) {
  const response = await fetch(`${apiUrl}/commits/${encodeURIComponent(ref)}`, {
    headers: { Accept: "application/vnd.github+json" },
  });
  const commit = (await response.json()) as GitHubCommit;
  if (!response.ok) {
    if (optional && response.status === 404) return null;
    throw new Error(commit.message || `GitHub could not resolve ${ref} to a commit.`);
  }
  if (!/^[0-9a-f]{40,64}$/i.test(commit.sha || "")) {
    throw new Error(commit.message || `GitHub returned an invalid commit for ${ref}.`);
  }
  return commit.sha;
}

async function inspectRepository(url: string) {
  const target = parseGitHubInput(url);
  if (!target) {
    throw new Error("Use a public GitHub repository, file, or folder URL.");
  }
  const response = await fetch(target.apiUrl, {
    headers: { Accept: "application/vnd.github+json" },
  });
  const repository = (await response.json()) as GitHubRepository;
  if (!response.ok) {
    throw new Error(repository.message || "GitHub could not find that public repository.");
  }
  if (repository.private) {
    throw new Error("Private repositories are not supported in this build.");
  }

  let commitSha = (await resolveCommit(target.apiUrl, repository.default_branch))!;
  let focusPath = "";
  let focusRef = "";
  if (target.kind !== "repository" && target.pathSegments.length > 0) {
    const defaultBranchSegments = repository.default_branch.split("/");
    const startsWithDefaultBranch = defaultBranchSegments.every(
      (segment, index) => target.pathSegments[index] === segment,
    );
    if (startsWithDefaultBranch) {
      focusPath = target.pathSegments.slice(defaultBranchSegments.length).join("/");
      focusRef = repository.default_branch;
    } else {
      for (let index = Math.max(1, target.pathSegments.length - 1); index >= 1; index -= 1) {
        const candidateRef = target.pathSegments.slice(0, index).join("/");
        const candidateCommit = await resolveCommit(target.apiUrl, candidateRef, true);
        if (candidateCommit) {
          commitSha = candidateCommit;
          focusPath = target.pathSegments.slice(index).join("/");
          focusRef = candidateRef;
          break;
        }
      }
    }
  }

  const summary: RepositorySummary = {
    fullName: repository.full_name,
    url: target.repoUrl,
    description: repository.description,
    language: repository.language,
    defaultBranch: repository.default_branch,
    stars: repository.stargazers_count,
    isPrivate: repository.private,
    commitSha,
  };
  const focusedScope = focusPath
    ? [
        `Focused path: ${focusPath}`,
        `GitHub ref from pasted URL: ${focusRef || repository.default_branch}`,
        `Pinned commit: ${commitSha}`,
        ...AUDIT_SCOPE,
      ]
    : AUDIT_SCOPE;
  return { repository: summary, scope: focusedScope };
}

function shortHash(value?: string) {
  return value ? value.slice(0, 10).toUpperCase() : "NONE";
}

function CampaignConsole() {
  const [admin, setAdmin] = useState<AdminState>({
    campaigns: [],
    jobs: [],
    peers: [],
    credits: { total: 0, allocations: [] },
    networkInfo: null,
    snapshots: {},
  });
  const [agentId, setAgentId] = useState("");
  const [repositoryUrl, setRepositoryUrl] = useState("");
  const [credits, setCredits] = useState("100");
  const [auditBrief, setAuditBrief] = useState("");
  const [attachmentText, setAttachmentText] = useState("");
  const [customSkillText, setCustomSkillText] = useState("");
  const [notice, setNotice] = useState("Native node not connected yet.");
  const [error, setError] = useState("");
  const [creating, setCreating] = useState(false);
  const [actionCampaignId, setActionCampaignId] = useState<string | null>(null);
  const [latestExport, setLatestExport] = useState<LatestExport | null>(null);

  const sortedJobs = useMemo(
    () => [...admin.jobs].sort((a, b) => b.createdAt - a.createdAt),
    [admin.jobs],
  );
  const sortedCampaigns = useMemo(
    () =>
      [...admin.campaigns].sort((a, b) =>
        b.updatedAt.localeCompare(a.updatedAt),
      ),
    [admin.campaigns],
  );

  async function refresh() {
    if (!isTauriRuntime()) {
      setNotice("Open campaign.html inside the native CYPHES/Tauri app to use the local node backend.");
      return;
    }
    const [campaigns, jobs, peers, networkInfo, creditSummary] = await Promise.all([
      invoke<ProtocolAuditCampaign[]>("list_protocol_campaigns"),
      invoke<AuditJob[]>("list_audits"),
      invoke<BackendPeerInfo[]>("get_peers"),
      invoke<NetworkInfo>("get_network_info"),
      invoke<CreditSummary>("get_credit_summary"),
    ]);
    const snapshotEntries = await Promise.all(
      campaigns.map(async (campaign) => {
        try {
          const snapshot = await invoke<CampaignReportSnapshot>("get_campaign_snapshot", {
            campaignId: campaign.campaignId,
          });
          return [campaign.campaignId, snapshot] as const;
        } catch {
          return null;
        }
      }),
    );
    setAgentId(networkInfo.agent_id);
    setAdmin({
      campaigns,
      jobs,
      peers,
      credits: creditSummary,
      networkInfo,
      snapshots: Object.fromEntries(
        snapshotEntries.filter(
          (entry): entry is [string, CampaignReportSnapshot] => Boolean(entry),
        ),
      ),
    });
  }

  useEffect(() => {
    let disposed = false;
    async function boot() {
      if (!isTauriRuntime()) {
        setNotice("Browser preview only. Campaign signing requires the native CYPHES app.");
        return;
      }
      const started = await invoke<StartNodeResponse>("start_node");
      if (disposed) return;
      setAgentId(started.agent_id);
      setNotice(`Campaign console linked to ${truncatePeerId(started.agent_id)}.`);
      await refresh();
    }
    boot().catch((caught) => setError(String(caught)));
    const timer = window.setInterval(() => {
      void refresh();
    }, 5000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, []);

  async function createCampaign(event: FormEvent) {
    event.preventDefault();
    setError("");
    setCreating(true);
    try {
      if (!isTauriRuntime()) {
        throw new Error("Campaign signing requires the native CYPHES app.");
      }
      const amount = Number(credits);
      if (!Number.isFinite(amount) || amount <= 0) {
        throw new Error("Enter an ATP Credits budget greater than zero.");
      }
      const inspected = await inspectRepository(repositoryUrl);
      await invoke<AuditJob>("create_audit", {
        repository: inspected.repository,
        compensation: credits,
        scope: inspected.scope,
        auditBriefText: auditBrief,
        attachmentText,
        customSkillText,
      });
      setRepositoryUrl("");
      setAuditBrief("");
      setAttachmentText("");
      setCustomSkillText("");
      setNotice(`Signed campaign for ${inspected.repository.fullName} and broadcast it to ${admin.peers.length} peer(s).`);
      await refresh();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setCreating(false);
    }
  }

  async function verifyCampaign(campaign: ProtocolAuditCampaign) {
    setError("");
    setActionCampaignId(campaign.campaignId);
    try {
      const snapshot =
        admin.snapshots[campaign.campaignId] ||
        (await invoke<CampaignReportSnapshot>("get_campaign_snapshot", {
          campaignId: campaign.campaignId,
        }));
      const verifiedIds = new Set(
        snapshot.verifications.map((item) => item.targetContributionId),
      );
      const pending = snapshot.contributions.filter(
        (item) => !verifiedIds.has(item.contributionId),
      );
      if (pending.length === 0) {
        throw new Error("No unverified contribution is available.");
      }
      const issued: CreditAllocation[] = [];
      for (const contribution of pending) {
        issued.push(
          ...(await invoke<CreditAllocation[]>("verify_campaign_contribution", {
            contributionId: contribution.contributionId,
            decision: "accepted",
            reasonCode: "COVERAGE_ACCEPTED",
            reason: "Contribution is bounded, signed, and useful for campaign coverage.",
          })),
        );
      }
      const total = issued.reduce((sum, item) => sum + item.total, 0);
      setNotice(
        `${pending.length} contribution${pending.length === 1 ? "" : "s"} accepted; ${total} ATP Credits issued and returned to worker.`,
      );
      await refresh();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setActionCampaignId(null);
    }
  }

  async function exportCampaign(campaign: ProtocolAuditCampaign) {
    setError("");
    setActionCampaignId(campaign.campaignId);
    try {
      const bundle = await invoke<ExportedReportBundle>("export_campaign_report", {
        campaignId: campaign.campaignId,
      });
      const reportPath = `${bundle.bundlePath.replace(/\/$/, "")}/report.md`;
      setLatestExport({
        campaignId: campaign.campaignId,
        bundlePath: bundle.bundlePath,
        reportPath,
      });
      setNotice(`Final audit report bundle exported. Finder opened report.md.`);
      await revealItemInDir(reportPath).catch(() => undefined);
      await refresh();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      setActionCampaignId(null);
    }
  }

  return (
    <main className="campaign-console">
      <header className="campaign-hero">
        <div>
          <span>CYPHES</span>
          <h1>Campaign Console</h1>
          <p>Protocol intake, network state, ATP proof trail, and developer envelopes.</p>
        </div>
        <div className="campaign-stat-grid">
          <div>
            <small>Peers</small>
            <strong>{admin.peers.length}</strong>
          </div>
          <div>
            <small>Campaigns</small>
            <strong>{admin.campaigns.length}</strong>
          </div>
          <div>
            <small>ATP</small>
            <strong>{admin.credits.total}</strong>
          </div>
          <div>
            <small>Agent</small>
            <strong>{truncatePeerId(agentId || "offline")}</strong>
          </div>
        </div>
      </header>

      <section className="campaign-admin-grid">
        <form className="campaign-admin-card" onSubmit={(event) => void createCampaign(event)}>
          <div className="section-heading">
            <span>Admin</span>
            <div>
              <h2>Create Protocol Campaign</h2>
              <p>Creates signed ATP work orders and broadcasts them to connected CYPHES nodes.</p>
            </div>
          </div>
          <label htmlFor="admin-repository-url">Repository, file, or folder URL</label>
          <div className="input-shell">
            <input
              id="admin-repository-url"
              onChange={(event) => setRepositoryUrl(event.currentTarget.value)}
              placeholder="https://github.com/owner/repo"
              spellCheck={false}
              type="url"
              value={repositoryUrl}
            />
          </div>
          <label htmlFor="admin-credits">ATP Credits budget</label>
          <div className="input-shell">
            <input
              id="admin-credits"
              min="1"
              onChange={(event) => setCredits(event.currentTarget.value)}
              step="1"
              type="number"
              value={credits}
            />
            <span>ATP Credits</span>
          </div>
          <label htmlFor="admin-brief">Audit brief</label>
          <div className="input-shell textarea-shell">
            <textarea
              id="admin-brief"
              onChange={(event) => setAuditBrief(event.currentTarget.value)}
              placeholder="Scope, concerns, bounty rules, protocol notes."
              spellCheck={false}
              value={auditBrief}
            />
          </div>
          <label htmlFor="admin-attachments">Attachments / docs text</label>
          <div className="input-shell textarea-shell">
            <textarea
              id="admin-attachments"
              onChange={(event) => setAttachmentText(event.currentTarget.value)}
              placeholder="Paste policy, protocol docs, or PDF excerpts."
              spellCheck={false}
              value={attachmentText}
            />
          </div>
          <details className="advanced-skill">
            <summary>Advanced SKILL.md overlay</summary>
            <div className="input-shell textarea-shell">
              <textarea
                onChange={(event) => setCustomSkillText(event.currentTarget.value)}
                placeholder="Optional custom methodology. Hash is recorded in receipts."
                spellCheck={false}
                value={customSkillText}
              />
            </div>
          </details>
          {error ? <div className="form-error">{error}</div> : null}
          <button className="primary-action" disabled={creating || !repositoryUrl.trim()} type="submit">
            {creating ? "Signing campaign" : "Sign and broadcast campaign"}
          </button>
        </form>

        <div className="campaign-admin-card proof-card">
          <div className="section-heading">
            <span>Network</span>
            <div>
              <h2>Birdseye State</h2>
              <p>{notice}</p>
            </div>
          </div>
          <div className="proof-grid">
            <div>
              <small>Relay</small>
              <strong>{admin.networkInfo?.relay_connected ? "connected" : "pending"}</strong>
            </div>
            <div>
              <small>Rendezvous</small>
              <strong>{admin.networkInfo?.rendezvous_registered ? "discoverable" : "pending"}</strong>
            </div>
            <div>
              <small>Bootstrap</small>
              <strong>{admin.networkInfo?.bootstrap_source || "local"}</strong>
            </div>
          </div>
          <h3>ATP Proof Log</h3>
          <div className="proof-log">
            {sortedJobs.length === 0 ? (
              <span>No ATP envelopes committed yet.</span>
            ) : (
              sortedJobs.map((job) => (
                <div key={job.id}>
                  <strong>{job.status}</strong>
                  <span>{job.repository.fullName}</span>
                  <code>{shortHash(job.lastEventHash)}</code>
                </div>
              ))
            )}
          </div>
        </div>
      </section>

      <section className="campaign-admin-card">
        <div className="section-heading">
          <span>Campaigns</span>
          <div>
            <h2>Protocol Events</h2>
            <p>Campaigns and ATP transactions remain separate inspectable objects.</p>
          </div>
        </div>
        <div className="campaign-admin-list">
          {sortedCampaigns.map((campaign) => {
            const snapshot = admin.snapshots[campaign.campaignId];
            const contributions = snapshot?.contributions.length || 0;
            const accepted =
              snapshot?.verifications.filter((item) => item.decision === "accepted").length || 0;
            const unverified = contributions - (snapshot?.verifications.length || 0);
            const workUnits = snapshot?.workUnits || [];
            return (
              <article className="campaign-admin-event" key={campaign.campaignId}>
                <div className="campaign-admin-event-main">
                  <div>
                    <strong>{campaign.protocolName}</strong>
                    <span>{campaign.repository.fullName}@{shortHash(campaign.repository.commitSha)}</span>
                  </div>
                  <code>{truncatePeerId(campaign.requesterAgentId)}</code>
                  <span>{campaign.status}</span>
                  <span>{campaign.skillPack.label}</span>
                </div>
                <div className="campaign-admin-metrics">
                  <span>{workUnits.length} work units</span>
                  <span>{contributions} contributions</span>
                  <span>{accepted} accepted</span>
                  <span className={unverified > 0 ? "needs-action" : ""}>
                    {unverified > 0 ? `${unverified} needs verification` : "verified queue clear"}
                  </span>
                </div>
                <div className="campaign-admin-work-units">
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
                    const latestVerification = unitVerifications[unitVerifications.length - 1];
                    return (
                      <div key={unit.workUnitId}>
                        <strong>{unit.title}</strong>
                        <span>{unit.status}</span>
                        <span>{unitContributions.length} contrib</span>
                        <span>
                          {latestVerification?.decision ||
                            (unitContributions.length > 0 ? "awaiting verifier" : "open")}
                        </span>
                      </div>
                    );
                  })}
                </div>
                <div className="campaign-admin-actions">
                  <button
                    disabled={actionCampaignId === campaign.campaignId || unverified === 0}
                    onClick={() => void verifyCampaign(campaign)}
                    type="button"
                  >
                    {unverified > 0 ? `Verify ${unverified}` : "Verify queue clear"}
                  </button>
                  <button
                    disabled={actionCampaignId === campaign.campaignId || contributions === 0}
                    onClick={() => void exportCampaign(campaign)}
                    type="button"
                  >
                    Export report
                  </button>
                </div>
                {latestExport?.campaignId === campaign.campaignId ? (
                  <div className="campaign-export-path">
                    <strong>Report exported</strong>
                    <code>{latestExport.reportPath}</code>
                    <span>Bundle: {latestExport.bundlePath}</span>
                  </div>
                ) : null}
              </article>
            );
          })}
          {sortedCampaigns.length === 0 ? <p>No campaigns committed yet.</p> : null}
        </div>
      </section>

      <section className="campaign-admin-card">
        <div className="section-heading">
          <span>Developer</span>
          <div>
            <h2>ATP Envelopes / Receipt Trail</h2>
            <p>Use this panel to inspect raw protocol mechanics outside the operator cockpit.</p>
          </div>
        </div>
        <div className="developer-event-table">
          {sortedJobs.map((job) => (
            <div key={job.transactionId}>
              <code>{job.transactionId}</code>
              <span>{job.currency}</span>
              <span>{job.deliveryState}</span>
              <span>{job.receiptHash ? shortHash(job.receiptHash) : "no receipt"}</span>
            </div>
          ))}
        </div>
      </section>

      {latestExport ? (
        <div className="notice report-notice">
          <strong>Report exported</strong>
          <code>{latestExport.reportPath}</code>
        </div>
      ) : null}
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("campaign-root") as HTMLElement).render(
  <React.StrictMode>
    <CampaignConsole />
  </React.StrictMode>,
);
