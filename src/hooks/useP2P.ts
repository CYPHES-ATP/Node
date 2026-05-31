import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";
import type { AgentMessage, BackendPeerInfo } from "@/types";

interface StartNodeResponse {
  peer_id: string;
  topic: string;
  listen_addrs: string[];
}

export interface BackendAgentMessage {
  msg_type: AgentMessage["msgType"];
  agent_id: string;
  name: string;
  capabilities: string[];
  endpoint?: string;
  timestamp: number;
  signature?: string;
  payload?: string;
  target_peer_id?: string;
  location?: string;
  source?: AgentMessage["source"];
}

export function fromBackendMessage(message: BackendAgentMessage): AgentMessage {
  return {
    id: `${message.agent_id}-${message.msg_type}-${message.timestamp}`,
    msgType: message.msg_type,
    agentId: message.agent_id,
    peerId: message.agent_id,
    name: message.name,
    capabilities: message.capabilities,
    endpoint: message.endpoint,
    timestamp: message.timestamp,
    signature: message.signature,
    payload: message.payload,
    targetPeerId: message.target_peer_id,
    location: message.location,
    source: message.source ?? "global",
  };
}

export function useP2P() {
  const myAgent = useCyphesStore((state) => state.myAgent);
  const setMyAgent = useCyphesStore((state) => state.setMyAgent);
  const addWireItem = useCyphesStore((state) => state.addWireItem);
  const updatePeer = useCyphesStore((state) => state.updatePeer);
  const setToast = useCyphesStore((state) => state.setToast);
  const pulseSync = useCyphesStore((state) => state.pulseSync);

  async function startNode() {
    if (!isTauriRuntime()) {
      const peerId = `12D3KooWLocal${Math.floor(Date.now() / 1000)}`;
      setMyAgent({ peerId, isOnline: true });
      return { peer_id: peerId, topic: "cyphes-v0.1-wire", listen_addrs: [] };
    }

    const response = await invoke<StartNodeResponse>("start_node");
    setMyAgent({ peerId: response.peer_id, isOnline: true });
    return response;
  }

  async function refreshPeers() {
    if (!isTauriRuntime()) return;

    const peers = await invoke<BackendPeerInfo[]>("get_peers");
    peers.forEach((peer) => {
      updatePeer(peer.peer_id, {
        peerId: peer.peer_id,
        name: peer.name ?? "DISCOVERED_PEER",
        capabilities: peer.capabilities,
        endpoint: peer.endpoint,
        lastSeen: peer.last_seen,
        status: "online",
        attestations: 0,
        tasksCompleted: 0,
        source: peer.source ?? "global",
      });
    });
  }

  async function broadcastAdvertise() {
    const timestamp = Date.now();
    const message: AgentMessage = {
      id: `${myAgent.peerId}-advertise-${timestamp}`,
      msgType: "advertise",
      agentId: myAgent.peerId,
      peerId: myAgent.peerId,
      name: myAgent.name,
      capabilities: myAgent.capabilities,
      endpoint: myAgent.openClawConnected ? "http://localhost:8080" : undefined,
      timestamp,
      payload: myAgent.openClawConnected
        ? "OpenClaw station is broadcasting live capability state."
        : "Manual station broadcast from CYPHES.",
      source: "local",
    };

    addWireItem(message);
    pulseSync();

    if (isTauriRuntime()) {
      await invoke("broadcast_advertise", {
        name: myAgent.name,
        capabilities: myAgent.capabilities,
        endpoint: message.endpoint,
        payload: message.payload,
      });
    }

    setToast(`Beacon sent with ${myAgent.capabilities.length} capabilities.`);
  }

  async function sendPing(targetPeerId: string, targetName: string) {
    const timestamp = Date.now();

    if (isTauriRuntime()) {
      await invoke("send_ping", {
        targetPeerId,
        message: `Greeting ${targetName} from ${myAgent.name}.`,
      });
    }

    addWireItem({
      id: `${myAgent.peerId}-ping-${timestamp}`,
      msgType: "ping",
      agentId: myAgent.peerId,
      peerId: myAgent.peerId,
      name: myAgent.name,
      capabilities: myAgent.capabilities,
      timestamp,
      payload: `Greeting ${targetName}.`,
      targetPeerId,
      source: "local",
    });
    pulseSync();

    window.setTimeout(() => {
      addWireItem({
        id: `${targetPeerId}-pong-${timestamp + 1200}`,
        msgType: "pong",
        agentId: targetPeerId,
        peerId: targetPeerId,
        name: targetName,
        capabilities: [],
        timestamp: timestamp + 1200,
        payload: "Pong received. Capability card acknowledged.",
        targetPeerId: myAgent.peerId,
        source: "global",
      });
      pulseSync();
    }, 1200);

    setToast(`Ping sent to ${targetName}.`);
  }

  return {
    startNode,
    refreshPeers,
    broadcastAdvertise,
    sendPing,
  };
}
