import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";
import type {
  AuditJob,
  BackendPeerInfo,
  LegacyAuditJob,
  RepositorySummary,
} from "@/types";

interface StartNodeResponse {
  peer_id: string;
  agent_id: string;
  protocol: string;
  listen_addrs: string[];
}

interface MigrationResult {
  migrated: number;
  skipped: number;
}

export function useP2P() {
  const setNodeOnline = useCyphesStore((state) => state.setNodeOnline);
  const setPeerCount = useCyphesStore((state) => state.setPeerCount);
  const replaceJobs = useCyphesStore((state) => state.replaceJobs);

  async function startNode() {
    if (!isTauriRuntime()) {
      setNodeOnline("browser-preview", "browser-preview");
      return {
        peer_id: "browser-preview",
        agent_id: "browser-preview",
        protocol: "preview",
        listen_addrs: [],
      };
    }

    const response = await invoke<StartNodeResponse>("start_node");
    setNodeOnline(response.peer_id, response.agent_id);
    return response;
  }

  async function refreshPeers() {
    if (!isTauriRuntime()) {
      setPeerCount(0);
      return [];
    }

    const peers = await invoke<BackendPeerInfo[]>("get_peers");
    setPeerCount(peers.length);
    return peers;
  }

  async function loadAudits() {
    if (!isTauriRuntime()) {
      replaceJobs([]);
      return [];
    }
    const jobs = await invoke<AuditJob[]>("list_audits");
    replaceJobs(jobs);
    return jobs;
  }

  async function migrateLegacyJobs(jobs: LegacyAuditJob[]) {
    if (!isTauriRuntime() || jobs.length === 0) {
      return { migrated: 0, skipped: 0 };
    }
    return invoke<MigrationResult>("migrate_legacy_jobs", { jobs });
  }

  async function createAudit(
    repository: RepositorySummary,
    compensation: string,
    scope: string[],
  ) {
    if (!isTauriRuntime()) {
      throw new Error("Audit requests can only be created in the native CYPHES app.");
    }
    const job = await invoke<AuditJob>("create_audit", {
      repository,
      compensation,
      scope,
    });
    await loadAudits();
    return job;
  }

  async function offerAudit(jobId: string) {
    if (!isTauriRuntime()) {
      throw new Error("Worker offers require the native CYPHES app.");
    }
    const job = await invoke<AuditJob>("offer_audit", { jobId });
    await loadAudits();
    return job;
  }

  async function acceptOffer(jobId: string) {
    if (!isTauriRuntime()) {
      throw new Error("Worker selection requires the native CYPHES app.");
    }
    const job = await invoke<AuditJob>("accept_offer", { jobId });
    await loadAudits();
    return job;
  }

  return {
    startNode,
    refreshPeers,
    loadAudits,
    migrateLegacyJobs,
    createAudit,
    offerAudit,
    acceptOffer,
  };
}
