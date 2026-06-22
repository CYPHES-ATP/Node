export type NodeStatus = "starting" | "online" | "error";
export type AuditStatus =
  | "discovered"
  | "negotiating"
  | "negotiated"
  | "routed"
  | "executing"
  | "settled"
  | "attested"
  | "rejected"
  | "revoked";
export type DeliveryState = "queued" | "acknowledged" | "received";

export interface RepositorySummary {
  fullName: string;
  url: string;
  description: string | null;
  language: string | null;
  defaultBranch: string;
  stars: number;
  isPrivate: boolean;
  commitSha: string;
}

export interface AuditJob {
  id: string;
  transactionId: string;
  repository: RepositorySummary;
  compensation: string;
  currency: string;
  scope: string[];
  status: AuditStatus;
  deliveryState: DeliveryState;
  requesterAgentId: string;
  workerAgentId?: string;
  createdAt: number;
  updatedAt: number;
  lastEventHash: string;
  contractHash?: string;
  resultHash?: string;
  receiptHash?: string;
  bundlePath?: string;
  acknowledgedPeers: number;
  origin: "local" | "remote";
}

export interface ProtocolAuditCampaign {
  profile: string;
  profileVersion: string;
  campaignId: string;
  protocolName: string;
  repository: {
    fullName: string;
    url: string;
    commitSha: string;
  };
  scopeText: string;
  bountyUrl?: string;
  impactsInScope: string[];
  outOfScope: string[];
  auditBriefHash?: string;
  auditBriefText?: string;
  requesterAgentId: string;
  status: string;
  createdAt: string;
  updatedAt: string;
}

export interface AuditWorkUnit {
  profile: string;
  profileVersion: string;
  workUnitId: string;
  campaignId: string;
  kind: string;
  title: string;
  instructions: string;
  expectedArtifacts: string[];
  status: string;
  createdAt: string;
}

export interface NodeContribution {
  contributionId: string;
  campaignId: string;
  workUnitId: string;
  workerAgentId: string;
  notesMarkdown: string;
  receiptHash: string;
  contributionHash: string;
  findings: Array<{
    id: string;
    title: string;
    severity: string;
    status: string;
    reportable: boolean;
  }>;
}

export interface VerificationResult {
  verificationId: string;
  campaignId: string;
  targetContributionId: string;
  verifierAgentId: string;
  decision: string;
  reasonCode: string;
  reason: string;
}

export interface CreditAllocation {
  allocationId: string;
  campaignId: string;
  contributionId: string;
  verificationId: string;
  receiverAgentId: string;
  contributionReceiptHash: string;
  total: number;
}

export interface CreditSummary {
  total: number;
  allocations: CreditAllocation[];
}

export interface CampaignReportSnapshot {
  campaign: ProtocolAuditCampaign;
  workUnits: AuditWorkUnit[];
  contributions: NodeContribution[];
  verifications: VerificationResult[];
  credits: CreditAllocation[];
}

export interface ExportedReportBundle {
  campaignId: string;
  bundlePath: string;
}

export interface LegacyAuditJob {
  id: string;
  repository: RepositorySummary;
  compensation: string;
  currency: string;
  scope: string[];
  requesterPeerId: string;
  createdAt: number;
}

export interface AtpAck {
  accepted: boolean;
  duplicate: boolean;
  event_hash: string;
  transaction_id: string;
  state?: string;
  receiver_agent_id: string;
  committed_at: string;
  reason_code?: string;
  reason?: string;
}

export interface BackendPeerInfo {
  peer_id: string;
  last_seen: number;
}

export interface NetworkInfo {
  peer_id: string;
  agent_id: string;
  protocol: string;
  listen_addrs: string[];
  relay_configured: boolean;
  relay_connected: boolean;
  rendezvous_registered: boolean;
  bootstrap_source?: string;
  connected_peers: number;
}
