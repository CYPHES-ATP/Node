import { listen } from "@tauri-apps/api/event";
import { useEffect, type ReactNode } from "react";
import { isTauriRuntime } from "@/lib/utils";
import { fromBackendMessage, type BackendAgentMessage, useP2P } from "@/hooks/useP2P";
import { useCyphesStore } from "@/store/useCyphesStore";

interface P2PProviderProps {
  children: ReactNode;
}

export function P2PProvider({ children }: P2PProviderProps) {
  const p2p = useP2P();
  const addWireItem = useCyphesStore((state) => state.addWireItem);
  const updatePeer = useCyphesStore((state) => state.updatePeer);
  const pulseSync = useCyphesStore((state) => state.pulseSync);
  const setToast = useCyphesStore((state) => state.setToast);

  useEffect(() => {
    let disposed = false;
    let cleanup: Array<() => void> = [];

    p2p
      .startNode()
      .then((node) => {
        if (!disposed) {
          setToast(`Node online on ${node.topic}.`);
        }
      })
      .catch((error) => {
        setToast(`P2P node fallback active: ${String(error)}`);
      });

    if (isTauriRuntime()) {
      Promise.all([
        listen<BackendAgentMessage>("p2p:advertise", (event) => {
          addWireItem(fromBackendMessage(event.payload));
          pulseSync();
        }),
        listen<BackendAgentMessage>("p2p:heartbeat", (event) => {
          addWireItem(fromBackendMessage(event.payload));
          pulseSync();
        }),
        listen<BackendAgentMessage>("p2p:ping", (event) => {
          addWireItem(fromBackendMessage(event.payload));
          pulseSync();
        }),
        listen<BackendAgentMessage>("p2p:pong", (event) => {
          addWireItem(fromBackendMessage(event.payload));
          pulseSync();
        }),
        listen<{ peer_id: string; source: "local" | "global" | "seed" }>("p2p:peer_connected", (event) => {
          updatePeer(event.payload.peer_id, {
            peerId: event.payload.peer_id,
            name: "DISCOVERED_PEER",
            capabilities: [],
            lastSeen: Date.now(),
            status: "online",
            attestations: 0,
            tasksCompleted: 0,
            source: event.payload.source,
          });
        }),
      ]).then((unlisteners) => {
        cleanup = unlisteners;
      });
    }

    const peerTimer = window.setInterval(() => {
      p2p.refreshPeers().catch(() => undefined);
    }, 8_000);

    return () => {
      disposed = true;
      window.clearInterval(peerTimer);
      cleanup.forEach((unlisten) => unlisten());
    };
  }, []);

  return children;
}
