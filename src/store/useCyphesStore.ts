import { create } from "zustand";
import { seedAgents, seedWire } from "@/data/seedAgents";
import type { Agent, AgentMessage, MyAgent, NetworkStats, WireSource } from "@/types";

interface CyphesState {
  myAgent: MyAgent;
  wire: AgentMessage[];
  peers: Record<string, Agent>;
  selectedPeerId: string | null;
  networkStats: NetworkStats;
  toast: string | null;
  setMyAgent: (agent: Partial<MyAgent>) => void;
  setOpenClaw: (connected: boolean, capabilities?: string[]) => void;
  addWireItem: (item: AgentMessage) => void;
  updatePeer: (peerId: string, data: Partial<Agent>) => void;
  selectPeer: (peerId: string | null) => void;
  forkCapabilities: (capabilities: string[]) => void;
  setToast: (toast: string | null) => void;
  pulseSync: () => void;
}

const seedPeers = seedAgents.reduce<Record<string, Agent>>((acc, agent) => {
  acc[agent.peerId] = agent;
  return acc;
}, {});

function nextStats(peers: Record<string, Agent>, wireCount: number): NetworkStats {
  return {
    localPeers: Object.values(peers).filter((peer) => peer.source === "local").length,
    globalPeers: Object.values(peers).filter((peer) => peer.source !== "local").length,
    messages: wireCount,
    sync: Math.min(96, 28 + wireCount * 4 + Object.keys(peers).length * 3),
  };
}

function normalizePeer(item: AgentMessage): Agent {
  return {
    peerId: item.peerId || item.agentId,
    name: item.name,
    capabilities: item.capabilities,
    endpoint: item.endpoint,
    location: item.location,
    lastSeen: item.timestamp,
    status: "online",
    attestations: item.attestations ?? 0,
    tasksCompleted: item.tasksCompleted ?? 0,
    tagline: item.payload,
    source: item.source,
  };
}

function shouldDedupe(existing: AgentMessage, incoming: AgentMessage) {
  return (
    existing.peerId === incoming.peerId &&
    existing.msgType === incoming.msgType &&
    Math.abs(existing.timestamp - incoming.timestamp) < 5000
  );
}

export const useCyphesStore = create<CyphesState>((set, get) => ({
  myAgent: {
    name: "OPENCLAW_LOCAL",
    peerId: "0xlocal...pending",
    capabilities: ["web-scrape", "code-gen", "summarize"],
    isOnline: false,
    openClawConnected: false,
    openClawStatus: "checking",
  },
  wire: seedWire,
  peers: seedPeers,
  selectedPeerId: seedAgents[0]?.peerId ?? null,
  networkStats: nextStats(seedPeers, seedWire.length),
  toast: null,

  setMyAgent: (agent) =>
    set((state) => ({
      myAgent: { ...state.myAgent, ...agent },
    })),

  setOpenClaw: (connected, capabilities) =>
    set((state) => ({
      myAgent: {
        ...state.myAgent,
        openClawConnected: connected,
        openClawStatus: connected ? "connected" : "missing",
        capabilities: connected && capabilities?.length ? capabilities : state.myAgent.capabilities,
      },
    })),

  addWireItem: (item) =>
    set((state) => {
      const source: WireSource = item.source ?? "global";
      const incoming = { ...item, source, id: item.id || `${item.peerId}-${item.timestamp}` };

      if (state.wire.some((existing) => shouldDedupe(existing, incoming))) {
        return state;
      }

      const wire = [incoming, ...state.wire].slice(0, 100);
      const peers =
        incoming.peerId === state.myAgent.peerId
          ? state.peers
          : {
              ...state.peers,
              [incoming.peerId]: {
                ...(state.peers[incoming.peerId] ?? normalizePeer(incoming)),
                ...normalizePeer(incoming),
              },
            };

      return {
        wire,
        peers,
        networkStats: nextStats(peers, wire.length),
      };
    }),

  updatePeer: (peerId, data) =>
    set((state) => {
      const existing = state.peers[peerId];
      const peers = {
        ...state.peers,
        [peerId]: {
          peerId,
          name: data.name ?? existing?.name ?? "UNKNOWN_AGENT",
          capabilities: data.capabilities ?? existing?.capabilities ?? [],
          lastSeen: data.lastSeen ?? existing?.lastSeen ?? Date.now(),
          status: data.status ?? existing?.status ?? "unknown",
          attestations: data.attestations ?? existing?.attestations ?? 0,
          tasksCompleted: data.tasksCompleted ?? existing?.tasksCompleted ?? 0,
          source: data.source ?? existing?.source ?? "global",
          endpoint: data.endpoint ?? existing?.endpoint,
          location: data.location ?? existing?.location,
          tagline: data.tagline ?? existing?.tagline,
          joinedAt: data.joinedAt ?? existing?.joinedAt,
        },
      };

      return {
        peers,
        networkStats: nextStats(peers, state.wire.length),
      };
    }),

  selectPeer: (peerId) => set({ selectedPeerId: peerId }),

  forkCapabilities: (capabilities) =>
    set((state) => ({
      myAgent: {
        ...state.myAgent,
        capabilities: Array.from(new Set([...state.myAgent.capabilities, ...capabilities])),
      },
      toast: "Capability list forked into My Station.",
    })),

  setToast: (toast) => set({ toast }),

  pulseSync: () => {
    const current = get().networkStats.sync;
    set((state) => ({
      networkStats: {
        ...state.networkStats,
        sync: Math.min(100, current + 7),
      },
    }));

    window.setTimeout(() => {
      set((state) => ({
        networkStats: {
          ...state.networkStats,
          sync: Math.max(36, state.networkStats.sync - 5),
        },
      }));
    }, 1200);
  },
}));
