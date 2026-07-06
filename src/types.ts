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

export interface InspectedRepository {
  repository: RepositorySummary;
  focusPath?: string;
  focusRef?: string;
}

export interface RepositoryTarget {
  fullName: string;
  url: string;
  commitSha: string;
}

export interface GitHubAccessStatus {
  authenticated: boolean;
  paused: boolean;
  retryAt?: string;
  message: string;
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
  skillPack: {
    skillPackId: string;
    version: string;
    hash: string;
    label: string;
  };
  attachments: Array<{
    attachmentId: string;
    label: string;
    mediaType: string;
    sha256: string;
    sizeBytes: number;
    text?: string;
  }>;
  customSkillHash?: string;
  customSkillLabel?: string;
  customSkillText?: string;
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
  claimedByAgentId?: string;
  claimId?: string;
  claimedAt?: string;
  createdAt: string;
}

export interface AuditWorkUnitClaim {
  profile: string;
  profileVersion: string;
  claimId: string;
  campaignId: string;
  workUnitId: string;
  requesterAgentId: string;
  workerAgentId: string;
  status: string;
  createdAt: string;
  expiresAt?: string;
  publicKeyBase64Url: string;
  claimHash: string;
  signature: string;
}

export interface CognitionProof {
  profile: string;
  profileVersion: string;
  proofId: string;
  contributionId: string;
  workerAgentId: string;
  proofHash: string;
  target: {
    campaignId: string;
    workUnitId: string;
    workUnitKind: string;
    workUnitTitle: string;
    protocolName?: string;
    repository?: RepositoryTarget;
    scopeHash?: string;
    authorizationHash?: string;
  };
  claim: {
    claimType: string;
    vulnerabilityClass: string;
    status: string;
    hypothesis: string;
  };
  evidence: {
    notesHash: string;
    artifactHashes: string[];
    findingCount: number;
    reportableFindingCount: number;
    coverageCount: number;
    coverageEvidenceCount: number;
    reproducibleSteps: string[];
  };
  quality: {
    parserFallback: boolean;
    structuredOutput: boolean;
    qualityMultiplier: number;
    tier: string;
  };
  settlement: {
    finalityRule: string;
    requiredIndependentVerifiers: number;
    settlementStatus: string;
    creditProfile: string;
    penaltyPolicy: string;
  };
}

export interface NodeContribution {
  contributionId: string;
  campaignId: string;
  workUnitId: string;
  workerAgentId: string;
  runtime?: {
    operator: string;
    adapter: string;
    model: string;
    modelMultiplier: number;
    toolPolicy: string[];
    connected: boolean;
    endpointClass?: string;
    skillHash?: string;
    inputHash?: string;
    outputHash?: string;
    tokensPerSecond?: number;
  };
  notesMarkdown: string;
  receiptHash: string;
  contributionHash: string;
  findings: Array<{
    id: string;
    title: string;
    severity: string;
    status: string;
    evidence?: string[];
    reportable: boolean;
  }>;
  coverage?: Array<{
    area: string;
    status: string;
    evidence: string[];
  }>;
  commands?: string[];
  cognitionProof?: CognitionProof;
  defenseProof?: CognitionProof;
}

export interface VerificationResult {
  verificationId: string;
  campaignId: string;
  targetContributionId: string;
  verifierAgentId: string;
  decision: string;
  reasonCode: string;
  reason: string;
  autonomousFinality?: {
    profile: string;
    profileVersion: string;
    rule: string;
    targetReceiptHash: string;
    targetProofHash?: string;
    decision: string;
    disposition: string;
    settlesImmediately: boolean;
    requiredIndependentVerifiers: number;
    verifierIndependent: boolean;
    qualityTier: string;
  };
}

export interface CreditAllocation {
  allocationId: string;
  campaignId: string;
  contributionId: string;
  verificationId: string;
  receiverAgentId: string;
  contributionReceiptHash: string;
  buckets?: {
    participation: number;
    verification: number;
    coverage: number;
    finding: number;
    bonusAllocationPlaceholder: number;
  };
  total: number;
  formula?: string;
}

export interface CreditSummary {
  total: number;
  allocations: CreditAllocation[];
  provisionalTotal: number;
  provisionalAllocations: CreditAllocation[];
}

export interface CampaignReportSnapshot {
  campaign: ProtocolAuditCampaign;
  workUnits: AuditWorkUnit[];
  claims: AuditWorkUnitClaim[];
  contributions: NodeContribution[];
  verifications: VerificationResult[];
  credits: CreditAllocation[];
}

export interface ExportedReportBundle {
  campaignId: string;
  bundlePath: string;
}

export interface LocalModelList {
  provider: string;
  providerLabel: string;
  connected: boolean;
  models: string[];
  message: string;
}

export interface GuardianTarget {
  targetId: string;
  protocolName: string;
  source: string[];
  category: string;
  chains: string[];
  tvlRiskRank: number;
  repoUrl: string;
  repoUrls: string[];
  contractPaths: string[];
  docsUrl?: string;
  securityUrl?: string;
  inScopeText?: string;
  outOfScopeText?: string;
  lastAuditedCommit?: string | null;
  lastObservedCommit?: string | null;
  contractCriticality: number;
  priorityScore: number;
  scopeText: string;
  auditBrief: string;
  creditBudget: number;
  cadence: string;
  tags: string[];
}

export interface AuditRuntimeProgress {
  campaignId: string;
  workUnitId: string;
  phase: string;
  progress: number;
  tokensPerSecond?: number;
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
