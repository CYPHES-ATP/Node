import { listen } from "@tauri-apps/api/event";
import { useEffect, type ReactNode } from "react";
import { isTauriRuntime } from "@/lib/utils";
import { useP2P } from "@/hooks/useP2P";
import {
  clearLegacyJobs,
  readLegacyJobs,
  useCyphesStore,
} from "@/store/useCyphesStore";
import type { AtpAck } from "@/types";

interface P2PProviderProps {
  children: ReactNode;
}

export function P2PProvider({ children }: P2PProviderProps) {
  const p2p = useP2P();
  const setNodeError = useCyphesStore((state) => state.setNodeError);
  const setNotice = useCyphesStore((state) => state.setNotice);

  useEffect(() => {
    let disposed = false;
    let cleanup: Array<() => void> = [];

    async function initialize() {
      if (isTauriRuntime()) {
        cleanup = await Promise.all([
          listen("atp:jobs_changed", () => {
            void p2p.loadAudits();
          }),
          listen<AtpAck>("atp:delivery_acknowledged", (event) => {
            const state = event.payload.state || "committed";
            setNotice(`Peer verified and committed ATP state: ${state}.`);
          }),
          listen<{ reason: string }>("atp:delivery_failed", (event) => {
            setNotice(`ATP delivery remains queued: ${event.payload.reason}`);
          }),
          listen("p2p:peer_connected", () => {
            void p2p.refreshPeers();
          }),
          listen("p2p:peer_disconnected", () => {
            void p2p.refreshPeers();
          }),
        ]);
      }

      await p2p.startNode();
      if (disposed) return;

      const legacyJobs = readLegacyJobs();
      if (legacyJobs.length > 0 && isTauriRuntime()) {
        const migration = await p2p.migrateLegacyJobs(legacyJobs);
        if (migration.skipped === 0) {
          clearLegacyJobs();
        } else {
          setNotice(
            `${migration.migrated} local request(s) migrated; ${migration.skipped} unverified record(s) left untouched.`,
          );
        }
      }

      await Promise.all([p2p.refreshPeers(), p2p.loadAudits()]);
    }

    initialize().catch((error) => {
      setNodeError(String(error));
    });

    const peerTimer = window.setInterval(() => {
      void p2p.refreshPeers();
    }, 5_000);

    return () => {
      disposed = true;
      window.clearInterval(peerTimer);
      cleanup.forEach((unlisten) => unlisten());
    };
  }, []);

  return children;
}
