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
          listen("audit:labor_changed", () => {
            void p2p.loadProtocolCampaigns();
            void p2p.refreshCreditSummary();
          }),
          listen<{ contributionId: string }>("audit:contribution_received", (event) => {
            void p2p.loadProtocolCampaigns();
            void p2p.refreshCreditSummary();
            setNotice(`Remote audit contribution received: ${event.payload.contributionId.slice(0, 22)}...`);
          }),
          listen<{ campaignId: string; protocolName: string }>("audit:campaign_received", (event) => {
            void p2p.loadProtocolCampaigns();
            setNotice(`Remote campaign received: ${event.payload.protocolName || event.payload.campaignId}.`);
          }),
          listen<{ campaignId: string }>("audit:campaign_acknowledged", () => {
            setNotice("Campaign accepted by a discovered CYPHES node.");
          }),
          listen<{ claimId: string }>("audit:work_unit_claimed", (event) => {
            void p2p.loadProtocolCampaigns();
            setNotice(`Work unit claimed by remote node: ${event.payload.claimId.slice(0, 22)}...`);
          }),
          listen<{ claimId: string }>("audit:work_unit_claim_acknowledged", (event) => {
            void p2p.loadProtocolCampaigns();
            setNotice(`Requester accepted work-unit claim: ${event.payload.claimId.slice(0, 22)}...`);
          }),
          listen<{ contributionId: string }>("audit:contribution_acknowledged", (event) => {
            void p2p.loadProtocolCampaigns();
            setNotice(`Requester accepted audit contribution: ${event.payload.contributionId.slice(0, 22)}...`);
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
        p2p.loadProtocolCampaigns(),
        p2p.refreshCreditSummary(),
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
      window.clearInterval(peerTimer);
      cleanup.forEach((unlisten) => unlisten());
    };
  }, []);

  return children;
}
