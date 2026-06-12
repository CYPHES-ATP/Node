import { create } from "zustand";
import type { AuditJob, LegacyAuditJob, NodeStatus } from "@/types";

export const LEGACY_STORAGE_KEY = "cyphes.audit-jobs.v1";

export function readLegacyJobs(): LegacyAuditJob[] {
  try {
    const value = window.localStorage.getItem(LEGACY_STORAGE_KEY);
    return value ? (JSON.parse(value) as LegacyAuditJob[]) : [];
  } catch {
    return [];
  }
}

export function clearLegacyJobs() {
  window.localStorage.removeItem(LEGACY_STORAGE_KEY);
}

interface CyphesState {
  nodeStatus: NodeStatus;
  nodeError: string | null;
  peerId: string;
  agentId: string;
  peerCount: number;
  jobs: AuditJob[];
  notice: string | null;
  setNodeOnline: (peerId: string, agentId: string) => void;
  setNodeError: (message: string) => void;
  setPeerCount: (count: number) => void;
  replaceJobs: (jobs: AuditJob[]) => void;
  setNotice: (notice: string | null) => void;
}

export const useCyphesStore = create<CyphesState>((set) => ({
  nodeStatus: "starting",
  nodeError: null,
  peerId: "",
  agentId: "",
  peerCount: 0,
  jobs: [],
  notice: null,

  setNodeOnline: (peerId, agentId) =>
    set({ nodeStatus: "online", nodeError: null, peerId, agentId }),
  setNodeError: (message) => set({ nodeStatus: "error", nodeError: message }),
  setPeerCount: (peerCount) => set({ peerCount }),
  replaceJobs: (jobs) => set({ jobs }),
  setNotice: (notice) => set({ notice }),
}));
