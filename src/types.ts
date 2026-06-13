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
  connected_peers: number;
}
