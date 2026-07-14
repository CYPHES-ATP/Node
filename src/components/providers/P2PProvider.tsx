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
    let dashboardRefreshTimer: number | null = null;
    let dashboardRefreshInflight = false;
    let dashboardRefreshQueued = false;

    function scheduleDashboardRefresh(delay = 350) {
      if (!isTauriRuntime() || disposed) return;
      dashboardRefreshQueued = true;
      if (dashboardRefreshTimer !== null) return;
      dashboardRefreshTimer = window.setTimeout(() => {
        dashboardRefreshTimer = null;
        if (dashboardRefreshInflight || disposed) return;
        dashboardRefreshQueued = false;
        dashboardRefreshInflight = true;
        p2p.refreshNetworkDashboard()
          .catch((error) => {
            if (!disposed) setNodeError(String(error));
          })
          .finally(() => {
            dashboardRefreshInflight = false;
            if (dashboardRefreshQueued && !disposed) {
              scheduleDashboardRefresh(500);
            }
          });
      }, delay);
    }

    async function initialize() {
      if (isTauriRuntime()) {
        cleanup = await Promise.all([
          listen("atp:jobs_changed", () => {
            void p2p.loadAudits();
          }),
          listen("audit:labor_changed", () => {
            scheduleDashboardRefresh();
          }),
          listen<{ contributionId: string }>("audit:contribution_received", (event) => {
            scheduleDashboardRefresh();
            setNotice(`Remote audit contribution received: ${event.payload.contributionId.slice(0, 22)}...`);
          }),
          listen<{ campaignId: string; protocolName: string }>("audit:campaign_received", (event) => {
            scheduleDashboardRefresh();
            setNotice(`Remote campaign received: ${event.payload.protocolName || event.payload.campaignId}.`);
          }),
          listen<{ campaignId: string }>("audit:campaign_acknowledged", () => {
            setNotice("Campaign accepted by a discovered CYPHES node.");
          }),
          listen<{ claimId: string }>("audit:work_unit_claimed", (event) => {
            scheduleDashboardRefresh();
            setNotice(`Work unit claimed by remote node: ${event.payload.claimId.slice(0, 22)}...`);
          }),
          listen<{ claimId: string }>("audit:work_unit_claim_acknowledged", (event) => {
            scheduleDashboardRefresh();
            setNotice(`Requester accepted work-unit claim: ${event.payload.claimId.slice(0, 22)}...`);
          }),
          listen<{ contributionId: string }>("audit:contribution_acknowledged", (event) => {
            scheduleDashboardRefresh();
            setNotice(`Requester accepted audit contribution: ${event.payload.contributionId.slice(0, 22)}...`);
          }),
          listen<{ verificationId: string; creditTotal: number }>("audit:verification_received", (event) => {
            scheduleDashboardRefresh();
            setNotice(`Verification received; ${event.payload.creditTotal} ATP Credits recorded.`);
          }),
          listen<{ verificationId: string; creditTotal: number }>("audit:network_verification_issued", (event) => {
            scheduleDashboardRefresh();
            setNotice(`Network verification issued; ${event.payload.creditTotal} ATP Credits recorded.`);
          }),
          listen<{ verificationId: string; creditTotal: number }>("audit:verification_acknowledged", (event) => {
            scheduleDashboardRefresh();
            setNotice(`Worker acknowledged verification ${event.payload.verificationId.slice(0, 22)}... for ${event.payload.creditTotal} ATP Credits.`);
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
          listen("p2p:listen_address", () => {
            void p2p.refreshNetworkInfo();
          }),
          listen("p2p:relay_ready", () => {
            void p2p.refreshNetworkInfo();
            setNotice("Public relay reservation is active.");
          }),
          listen<{ discovered: number }>("p2p:rendezvous_discovered", (event) => {
            void p2p.refreshNetworkInfo();
            if (event.payload.discovered > 0) {
              setNotice(
                `Internet discovery found ${event.payload.discovered} CYPHES node(s).`,
              );
            }
          }),
          listen("p2p:rendezvous_registered", () => {
            void p2p.refreshNetworkInfo();
            setNotice("This node is discoverable on the CYPHES internet network.");
          }),
          listen<{ bundlePath: string }>("atp:receipt_committed", (event) => {
            void p2p.loadAudits();
            setNotice(`ATP transaction receipt exported to ${event.payload.bundlePath}.`);
          }),
          listen("atp:result_received", () => {
            void p2p.loadAudits();
            setNotice("Signed worker result received and verified.");
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

      await Promise.all([
        p2p.refreshPeers(),
        p2p.refreshNetworkInfo(),
        p2p.loadAudits(),
        p2p.refreshNetworkDashboard(),
      ]);
    }

    initialize().catch((error) => {
      setNodeError(String(error));
    });

    const peerTimer = window.setInterval(() => {
      void p2p.refreshPeers();
      void p2p.refreshNetworkInfo();
    }, 5_000);

    return () => {
      disposed = true;
      if (dashboardRefreshTimer !== null) {
        window.clearTimeout(dashboardRefreshTimer);
      }
      window.clearInterval(peerTimer);
      cleanup.forEach((unlisten) => unlisten());
    };
  }, []);

  return children;
}
