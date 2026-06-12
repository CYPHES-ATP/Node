import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  ArrowRight,
  Check,
  CircleDollarSign,
  Database,
  Github,
  LoaderCircle,
  ShieldCheck,
  Users,
} from "lucide-react";
import { TitleBar } from "@/components/layout/TitleBar";
import { P2PProvider } from "@/components/providers/P2PProvider";
import { useP2P } from "@/hooks/useP2P";
import { isTauriRuntime, truncatePeerId } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";
import type { AuditJob, RepositorySummary } from "@/types";

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

function repositoryApiUrl(value: string) {
  const normalized = value.trim().replace(/\.git$/, "").replace(/\/+$/, "");
  const match = normalized.match(/^https:\/\/github\.com\/([^/]+)\/([^/]+)$/i);
  if (!match) return null;
  return `https://api.github.com/repos/${encodeURIComponent(match[1])}/${encodeURIComponent(match[2])}`;
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
  const [formError, setFormError] = useState<string | null>(null);

  const nodeStatus = useCyphesStore((state) => state.nodeStatus);
  const nodeError = useCyphesStore((state) => state.nodeError);
  const agentId = useCyphesStore((state) => state.agentId);
  const peerCount = useCyphesStore((state) => state.peerCount);
  const jobs = useCyphesStore((state) => state.jobs);
  const notice = useCyphesStore((state) => state.notice);
  const setNotice = useCyphesStore((state) => state.setNotice);

  const sortedJobs = useMemo(
    () => [...jobs].sort((a, b) => b.createdAt - a.createdAt),
    [jobs],
  );

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 5_000);
    return () => window.clearTimeout(timer);
  }, [notice, setNotice]);

  async function inspectRepository(url: string) {
    const apiUrl = repositoryApiUrl(url);
    if (!apiUrl) {
      throw new Error("Use a public GitHub repository URL, for example https://github.com/owner/repo.");
    }

    const response = await fetch(apiUrl, {
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

    const commitResponse = await fetch(
      `${apiUrl}/commits/${encodeURIComponent(repository.default_branch)}`,
      {
        headers: {
          Accept: "application/vnd.github+json",
        },
      },
    );
    const commit = (await commitResponse.json()) as GitHubCommit;
    if (!commitResponse.ok || !/^[0-9a-f]{40,64}$/i.test(commit.sha || "")) {
      throw new Error(commit.message || "GitHub could not resolve the default branch to a commit.");
    }

    return toRepositorySummary(repository, commit.sha);
  }

  async function createAudit(event: FormEvent) {
    event.preventDefault();
    setFormError(null);

    const amount = Number(compensation);
    if (!Number.isFinite(amount) || amount <= 0) {
      setFormError("Enter a proposed compensation greater than zero.");
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
      const repository = await inspectRepository(repositoryUrl);
      const job = await p2p.createAudit(
        repository,
        amount.toString(),
        AUDIT_SCOPE,
      );
      setRepositoryUrl("");
      setNotice(
        peerCount > 0
          ? `ATP request signed, committed, and sent to ${peerCount} local ${peerCount === 1 ? "peer" : "peers"} for verification.`
          : `ATP request signed and committed to SQLite. It is queued with no peer receipt yet.`,
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

    let outcome = deliveryLabel(job);
    if (job.status === "negotiating") {
      outcome = isMine
        ? `Offer from ${truncatePeerId(job.workerAgentId || "")}`
        : "Offer committed, awaiting requester";
    } else if (job.status === "negotiated") {
      outcome = `Worker selected: ${truncatePeerId(job.workerAgentId || "")}`;
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
            <span>Request / ACK</span>
            <strong>{peerCount} LAN {peerCount === 1 ? "peer" : "peers"}</strong>
          </div>
          <div>
            <CircleDollarSign size={15} />
            <span>Payment rail</span>
            <strong className="warning">Not connected</strong>
          </div>
        </section>

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
                <h2>Create an audit request</h2>
                <p>Public GitHub repositories only.</p>
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

              <label htmlFor="compensation">Proposed compensation</label>
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
                  <span>USDC</span>
                </div>
                <p>Terms only. No funds are escrowed or transferred in this build.</p>
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
                {submitting ? "Checking repository" : "Sign and post request"}
                {!submitting ? <ArrowRight size={16} /> : null}
              </button>
            </form>
          </section>

          <section className="panel jobs-panel">
            <div className="section-heading">
              <span>02</span>
              <div>
                <h2>ATP transactions</h2>
                <p>Only signed events committed by this node.</p>
              </div>
            </div>

            <div className="jobs-list">
              {sortedJobs.length === 0 ? (
                <div className="empty-state">
                  <Github size={24} />
                  <strong>No committed audit requests</strong>
                  <span>Post a repository or connect another CYPHES node on the same network.</span>
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

        <footer>
          <span>ATP v0.3 envelopes</span>
          <span>Ed25519 identity proof</span>
          <span>SQLite event chain</span>
          <span>LAN request / ACK</span>
          <span>No payment rail</span>
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
