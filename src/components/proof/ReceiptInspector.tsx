import { useEffect, useMemo, useState } from "react";
import {
  Braces,
  Check,
  CircleDashed,
  Fingerprint,
  ShieldCheck,
  TriangleAlert,
  X,
} from "lucide-react";
import type {
  AuditWorkUnit,
  CampaignReportSnapshot,
  CreditAllocation,
  NodeContribution,
  ProtocolAuditCampaign,
  VerificationResult,
} from "@/types";

export type ReceiptInspectorMode = "verified" | "pending" | "penalized";

interface ReceiptInspectorProps {
  campaigns: ProtocolAuditCampaign[];
  mode: ReceiptInspectorMode;
  onClose: () => void;
  onModeChange: (mode: ReceiptInspectorMode) => void;
  snapshots: Record<string, CampaignReportSnapshot>;
}

interface ReceiptRecord {
  campaign: ProtocolAuditCampaign;
  contribution: NodeContribution;
  credit?: CreditAllocation;
  status: ReceiptInspectorMode;
  verification?: VerificationResult;
  workUnit?: AuditWorkUnit;
}

const FILTERS: Array<{ label: string; mode: ReceiptInspectorMode }> = [
  { label: "Verified", mode: "verified" },
  { label: "Pending", mode: "pending" },
  { label: "Penalized", mode: "penalized" },
];

function shortHash(value?: string, length = 12) {
  if (!value) return "Not available";
  return value.replace(/^sha256:/, "").slice(0, length).toUpperCase();
}

function shortIdentity(value?: string) {
  if (!value) return "Unknown identity";
  if (value.length <= 22) return value;
  return `${value.slice(0, 11)}...${value.slice(-7)}`;
}

function contributionStatus(
  contribution: NodeContribution,
  verification?: VerificationResult,
): ReceiptInspectorMode {
  const proof = contribution.cognitionProof ?? contribution.defenseProof;
  if (proof?.quality.parserFallback || (verification && verification.decision !== "accepted")) {
    return "penalized";
  }
  return verification?.decision === "accepted" ? "verified" : "pending";
}

function buildReceiptRecords(
  campaigns: ProtocolAuditCampaign[],
  snapshots: Record<string, CampaignReportSnapshot>,
) {
  return campaigns.flatMap((campaign) => {
    const snapshot = snapshots[campaign.campaignId];
    if (!snapshot) return [];
    const verificationByContribution = new Map(
      snapshot.verifications.map((verification) => [verification.targetContributionId, verification]),
    );
    const creditByContribution = new Map(
      snapshot.credits.map((credit) => [credit.contributionId, credit]),
    );
    const workUnitById = new Map(
      snapshot.workUnits.map((workUnit) => [workUnit.workUnitId, workUnit]),
    );

    return [...snapshot.contributions].reverse().map((contribution): ReceiptRecord => {
      const verification = verificationByContribution.get(contribution.contributionId);
      return {
        campaign,
        contribution,
        verification,
        credit: creditByContribution.get(contribution.contributionId),
        workUnit: workUnitById.get(contribution.workUnitId),
        status: contributionStatus(contribution, verification),
      };
    });
  });
}

function StageState({ complete }: { complete: boolean }) {
  return complete ? (
    <Check aria-hidden="true" size={13} />
  ) : (
    <CircleDashed aria-hidden="true" size={13} />
  );
}

export function ReceiptInspector({
  campaigns,
  mode,
  onClose,
  onModeChange,
  snapshots,
}: ReceiptInspectorProps) {
  const records = useMemo(
    () => buildReceiptRecords(campaigns, snapshots),
    [campaigns, snapshots],
  );
  const filteredRecords = useMemo(
    () => records.filter((record) => record.status === mode),
    [mode, records],
  );
  const visibleRecords = filteredRecords.slice(0, 10);
  const [selectedContributionId, setSelectedContributionId] = useState("");

  useEffect(() => {
    if (!visibleRecords.some((record) => record.contribution.contributionId === selectedContributionId)) {
      setSelectedContributionId(visibleRecords[0]?.contribution.contributionId || "");
    }
  }, [mode, selectedContributionId, visibleRecords]);

  const selected =
    visibleRecords.find(
      (record) => record.contribution.contributionId === selectedContributionId,
    ) || visibleRecords[0];
  const proof = selected?.contribution.cognitionProof ?? selected?.contribution.defenseProof;
  const independentlySigned = Boolean(
    selected?.verification &&
      selected.verification.verifierAgentId !== selected.contribution.workerAgentId,
  );
  const finalityAccepted = selected?.verification?.decision === "accepted";
  const stageChecks = selected
    ? [
        Boolean(selected.campaign.repository.commitSha),
        Boolean(proof),
        Boolean(selected.contribution.receiptHash),
        independentlySigned && finalityAccepted,
        Boolean(selected.credit),
      ]
    : [];
  const completedStages = stageChecks.filter(Boolean).length;

  return (
    <section
      aria-labelledby="receipt-inspector-title"
      className="receipt-inspector"
      id="receipt-inspector"
    >
      <header className="receipt-inspector-header">
        <div>
          <span className="receipt-inspector-kicker">Proof pipeline / testnet receipts</span>
          <h2 id="receipt-inspector-title">Receipt Inspector</h2>
          <p>
            Trace a local-model contribution from pinned work through signed evidence,
            identity-level finality, and ATP accounting.
          </p>
        </div>
        <button aria-label="Close receipt inspector" className="inspector-close" onClick={onClose} type="button">
          <X aria-hidden="true" size={17} />
        </button>
      </header>

      <div aria-label="Receipt filters" className="receipt-filters" role="tablist">
        {FILTERS.map((filter) => {
          const count = records.filter((record) => record.status === filter.mode).length;
          return (
            <button
              aria-controls="receipt-inspector-panel"
              aria-selected={mode === filter.mode}
              className={mode === filter.mode ? "is-active" : undefined}
              key={filter.mode}
              onClick={() => onModeChange(filter.mode)}
              role="tab"
              type="button"
            >
              <span>{filter.label}</span>
              <strong>{count}</strong>
            </button>
          );
        })}
      </div>

      <div className="receipt-inspector-body" id="receipt-inspector-panel" role="tabpanel">
        {visibleRecords.length > 0 ? (
          <>
            <nav aria-label={`${mode} receipt list`} className="receipt-list">
              <div className="receipt-list-heading">
                <span>Recent {mode}</span>
                <small>
                  {Math.min(visibleRecords.length, 10)} of {filteredRecords.length}
                </small>
              </div>
              {visibleRecords.map((record) => (
                <button
                  aria-current={
                    record.contribution.contributionId === selected?.contribution.contributionId
                      ? "true"
                      : undefined
                  }
                  className={
                    record.contribution.contributionId === selected?.contribution.contributionId
                      ? "is-selected"
                      : undefined
                  }
                  key={record.contribution.contributionId}
                  onClick={() => setSelectedContributionId(record.contribution.contributionId)}
                  type="button"
                >
                  <span>{record.campaign.protocolName}</span>
                  <strong>{record.workUnit?.title || "Audit contribution"}</strong>
                  <code>{shortHash(record.contribution.receiptHash)}</code>
                </button>
              ))}
            </nav>

            {selected ? (
              <article className="receipt-detail">
                <div className="receipt-detail-heading">
                  <div>
                    <span className={`receipt-state receipt-state-${selected.status}`}>
                      {selected.status === "verified" ? (
                        <ShieldCheck aria-hidden="true" size={13} />
                      ) : selected.status === "penalized" ? (
                        <TriangleAlert aria-hidden="true" size={13} />
                      ) : (
                        <CircleDashed aria-hidden="true" size={13} />
                      )}
                      {selected.status}
                    </span>
                    <h3>{selected.campaign.protocolName}</h3>
                    <p>{selected.campaign.repository.fullName}</p>
                  </div>
                  <div className="proof-score" aria-label={`${completedStages} of 5 proof stages complete`}>
                    <strong>{completedStages}/5</strong>
                    <span>stages</span>
                  </div>
                </div>

                <div className="proof-stage-grid">
                  <section className={stageChecks[0] ? "is-complete" : undefined}>
                    <div>
                      <span>01</span>
                      <StageState complete={stageChecks[0]} />
                    </div>
                    <h4>Work committed</h4>
                    <dl>
                      <div><dt>Unit</dt><dd>{selected.workUnit?.title || selected.contribution.workUnitId}</dd></div>
                      <div><dt>Commit</dt><dd><code>{shortHash(selected.campaign.repository.commitSha)}</code></dd></div>
                      <div><dt>Status</dt><dd>{selected.workUnit?.status || "submitted"}</dd></div>
                    </dl>
                  </section>

                  <section className={stageChecks[1] && stageChecks[2] ? "is-complete" : undefined}>
                    <div>
                      <span>02</span>
                      <StageState complete={stageChecks[1] && stageChecks[2]} />
                    </div>
                    <h4>Evidence committed</h4>
                    <dl>
                      <div><dt>Model</dt><dd>{selected.contribution.runtime?.model || "Not declared"}</dd></div>
                      <div><dt>Quality</dt><dd>{proof?.quality.tier || "Unclassified"}</dd></div>
                      <div><dt>Receipt</dt><dd><code>{shortHash(selected.contribution.receiptHash)}</code></dd></div>
                    </dl>
                  </section>

                  <section className={stageChecks[3] ? "is-complete" : undefined}>
                    <div>
                      <span>03</span>
                      <StageState complete={stageChecks[3]} />
                    </div>
                    <h4>Identity finality</h4>
                    <dl>
                      <div><dt>Decision</dt><dd>{selected.verification?.decision || "Awaiting verifier"}</dd></div>
                      <div><dt>Verifier</dt><dd>{shortIdentity(selected.verification?.verifierAgentId)}</dd></div>
                      <div><dt>Independent ID</dt><dd>{independentlySigned ? "Different key" : "Not established"}</dd></div>
                    </dl>
                    <p className="proof-caveat">
                      Testnet finality confirms a distinct signing identity. It does not yet prove
                      bonded quorum or reproduced model computation.
                    </p>
                  </section>

                  <section className={stageChecks[4] ? "is-complete" : undefined}>
                    <div>
                      <span>04</span>
                      <StageState complete={stageChecks[4]} />
                    </div>
                    <h4>ATP accounting</h4>
                    <dl>
                      <div><dt>Total</dt><dd>{selected.credit ? `${selected.credit.total} ATP` : "Not allocated"}</dd></div>
                      <div><dt>Coverage</dt><dd>{selected.credit?.buckets?.coverage ?? 0}</dd></div>
                      <div><dt>Finding</dt><dd>{selected.credit?.buckets?.finding ?? 0}</dd></div>
                    </dl>
                    <p className="proof-caveat">Receipt-derived testnet accounting—not a token, escrow, or payable balance.</p>
                  </section>
                </div>

                <details className="raw-receipt">
                  <summary>
                    <Braces aria-hidden="true" size={14} />
                    Inspect signed object data
                  </summary>
                  <pre>{JSON.stringify({
                    campaign: {
                      campaignId: selected.campaign.campaignId,
                      repository: selected.campaign.repository,
                    },
                    workUnit: selected.workUnit,
                    contribution: selected.contribution,
                    verification: selected.verification,
                    credit: selected.credit,
                  }, null, 2)}</pre>
                </details>
              </article>
            ) : null}
          </>
        ) : (
          <div className="receipt-empty-state">
            <Fingerprint aria-hidden="true" size={28} />
            <strong>No {mode} receipts yet</strong>
            <p>
              Pinned work, evidence commitments, verifier signatures, and receipt-derived ATP
              will appear here as the node participates.
            </p>
            <ol aria-label="Receipt lifecycle">
              <li>Commit work</li>
              <li>Sign evidence</li>
              <li>Verify receipt</li>
              <li>Derive ATP</li>
            </ol>
          </div>
        )}
      </div>
    </section>
  );
}
