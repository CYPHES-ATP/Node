export type AgentStatus = "online" | "offline" | "unknown";
export type WireSource = "local" | "global" | "seed";
export type WireType = "advertise" | "heartbeat" | "ping" | "pong";

export interface Agent {
  peerId: string;
  name: string;
  capabilities: string[];
  endpoint?: string;
  location?: string;
  lastSeen: number;
  joinedAt?: number;
  status: AgentStatus;
  attestations: number;
  tasksCompleted: number;
  tagline?: string;
  source: WireSource;
}

export interface AgentMessage {
  id: string;
  msgType: WireType;
  agentId: string;
  peerId: string;
  name: string;
  capabilities: string[];
  endpoint?: string;
  timestamp: number;
  signature?: string;
  payload?: string;
  targetPeerId?: string;
  location?: string;
  attestations?: number;
  tasksCompleted?: number;
  source: WireSource;
}

export interface MyAgent {
  name: string;
  peerId: string;
  capabilities: string[];
  isOnline: boolean;
  openClawConnected: boolean;
  openClawStatus: "checking" | "connected" | "missing";
}

export interface NetworkStats {
  localPeers: number;
  globalPeers: number;
  messages: number;
  sync: number;
}

export interface BackendPeerInfo {
  peer_id: string;
  name?: string;
  capabilities: string[];
  endpoint?: string;
  last_seen: number;
  source: WireSource;
}
