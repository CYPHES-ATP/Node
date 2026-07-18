import { invoke } from "@tauri-apps/api/core";
import guardianTargetIndex from "../../protocol/targets/guardian-target-index.json";
import { isTauriRuntime } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";
import type {
  AuditJob,
  BackendPeerInfo,
  CampaignReportSnapshot,
  CreditAllocation,
  CreditSummary,
  ExportedReportBundle,
  GuardianTarget,
  GitHubAccessStatus,
  InspectedRepository,
  LegacyAuditJob,
  LocalModelList,
  NetworkDashboardSummary,
  NetworkInfo,
  NetworkVerificationIssued,
  NodeContribution,
  ProtocolAuditCampaign,
  RepositorySummary,
  AuditWorkUnitClaim,
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
  const setNetworkInfo = useCyphesStore((state) => state.setNetworkInfo);
  const replaceJobs = useCyphesStore((state) => state.replaceJobs);
  const replaceCampaigns = useCyphesStore((state) => state.replaceCampaigns);
  const setNetworkSummary = useCyphesStore((state) => state.setNetworkSummary);
  const setCreditSummary = useCyphesStore((state) => state.setCreditSummary);

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

  async function refreshNetworkInfo() {
    if (!isTauriRuntime()) return null;
    const info = await invoke<NetworkInfo>("get_network_info");
    setNetworkInfo(info);
    return info;
  }

  async function connectPeer(address: string) {
    if (!isTauriRuntime()) {
      throw new Error("Peer connections require the native CYPHES app.");
    }
    await invoke("connect_peer", { address });
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

  async function loadProtocolCampaigns() {
    if (!isTauriRuntime()) {
      replaceCampaigns([]);
      setNetworkSummary(null);
      return [];
    }
    const campaigns = await invoke<ProtocolAuditCampaign[]>("list_protocol_campaigns");
    replaceCampaigns(campaigns);
    return campaigns;
  }

  async function refreshNetworkDashboard() {
    if (!isTauriRuntime()) {
      replaceCampaigns([]);
      setNetworkSummary(null);
      const empty = { total: 0, allocations: [], provisionalTotal: 0, provisionalAllocations: [] };
      setCreditSummary(empty);
      return null;
    }
    const dashboard = await invoke<NetworkDashboardSummary>("get_network_dashboard");
    replaceCampaigns(dashboard.campaigns);
    setNetworkSummary(dashboard.progress);
    setCreditSummary(dashboard.creditSummary);
    return dashboard;
  }

  async function getCampaignSnapshot(campaignId: string) {
    if (!isTauriRuntime()) {
      throw new Error("Campaign snapshots require the native CYPHES app.");
    }
    return invoke<CampaignReportSnapshot>("get_campaign_live_snapshot", { campaignId });
  }

  async function listLocalModelProviders() {
    if (!isTauriRuntime()) return [];
    return invoke<LocalModelList[]>("list_local_model_providers");
  }

  async function listGuardianTargets() {
    if (!isTauriRuntime()) return guardianTargetIndex.targets as GuardianTarget[];
    return invoke<GuardianTarget[]>("list_guardian_targets");
  }

  async function getGitHubAccessStatus() {
    if (!isTauriRuntime()) {
      return {
        authenticated: false,
        paused: false,
        message: "Browser preview does not use the native GitHub access layer.",
      } satisfies GitHubAccessStatus;
    }
    return invoke<GitHubAccessStatus>("get_github_access_status");
  }

  async function inspectGithubRepository(url: string) {
    if (!isTauriRuntime()) {
      throw new Error("GitHub inspection requires the native CYPHES app.");
    }
    return invoke<InspectedRepository>("inspect_github_repository", { url });
  }

  async function listLocalModelModels(provider: string) {
    if (!isTauriRuntime()) {
      return {
        provider,
        providerLabel: provider,
        connected: false,
        models: [],
        message: "Local model discovery requires the native CYPHES app.",
      };
    }
    return invoke<LocalModelList>("list_local_model_models", { provider });
  }

  async function refreshCreditSummary() {
    if (!isTauriRuntime()) {
      const empty = { total: 0, allocations: [], provisionalTotal: 0, provisionalAllocations: [] };
      setCreditSummary(empty);
      return empty;
    }
    const summary = await invoke<CreditSummary>("get_credit_summary");
    setCreditSummary(summary);
    return summary;
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
    auditBriefText = "",
    attachmentText = "",
    customSkillText = "",
  ) {
    if (!isTauriRuntime()) {
      throw new Error("Audit requests can only be created in the native CYPHES app.");
    }
    const job = await invoke<AuditJob>("create_audit", {
      repository,
      compensation,
      scope,
      auditBriefText,
      attachmentText,
      customSkillText,
    });
    await Promise.all([loadAudits(), refreshNetworkDashboard()]);
    return job;
  }

  async function createProtocolCampaign(
    repository: RepositorySummary,
    protocolName: string,
    scopeText: string,
    creditBudget: string,
    auditBriefText = "",
    attachmentText = "",
    customSkillText = "",
  ) {
    if (!isTauriRuntime()) {
      throw new Error("Protocol campaigns can only be created in the native CYPHES app.");
    }
    const campaign = await invoke<ProtocolAuditCampaign>("create_protocol_campaign", {
      request: {
        protocolName,
        repository,
        scopeText,
        bountyUrl: "",
        impactsInScope: [
          "Evidence-backed repository risk",
          "Reportable security impact if proven",
        ],
        outOfScope: [
          "Best-practice-only notes",
          "Claims without reproducible evidence",
          "Production testing or unauthorized external interaction",
        ],
        auditBriefText: [
          `ATP Credits budget: ${creditBudget}. Credits are off-chain receipt-backed accounting only.`,
          auditBriefText.trim(),
        ].filter(Boolean).join("\n\n"),
        attachmentText,
        customSkillText,
      },
    });
    await refreshNetworkDashboard();
    return campaign;
  }

  async function claimCampaignWorkUnit(campaignId: string, workUnitId: string) {
    const claim = await invoke<AuditWorkUnitClaim>("claim_campaign_work_unit", {
      campaignId,
      workUnitId,
    });
    await refreshNetworkDashboard();
    return claim;
  }

  async function runClaimedWorkUnit(
    campaignId: string,
    workUnitId: string,
    provider: string,
    model: string,
    maxRuntimeSeconds?: number,
  ) {
    const contribution = await invoke<NodeContribution>("run_claimed_work_unit", {
      campaignId,
      workUnitId,
      provider,
      model,
      maxRuntimeSeconds,
    });
    await refreshNetworkDashboard();
    return contribution;
  }

  async function recordCampaignContribution(
    campaignId: string,
    workUnitId: string,
    notesMarkdown: string,
  ) {
    const contribution = await invoke<NodeContribution>("record_campaign_contribution", {
      campaignId,
      workUnitId,
      notesMarkdown,
    });
    await refreshNetworkDashboard();
    return contribution;
  }

  async function runCampaignAuditSkill(
    campaignId: string,
    workUnitId: string,
    provider: string,
    model: string,
  ) {
    const contribution = await invoke<NodeContribution>("run_campaign_audit_skill", {
      campaignId,
      workUnitId,
      provider,
      model,
    });
    await refreshNetworkDashboard();
    return contribution;
  }

  async function runCampaignAuditPipeline(
    campaignId: string,
    provider: string,
    model: string,
  ) {
    const contributions = await invoke<NodeContribution[]>("run_campaign_audit_pipeline", {
      campaignId,
      provider,
      model,
    });
    await refreshNetworkDashboard();
    return contributions;
  }

  async function runAcceptedAuditSkill(
    jobId: string,
    provider: string,
    model: string,
  ) {
    const contribution = await invoke<NodeContribution>("run_accepted_audit_skill", {
      jobId,
      provider,
      model,
    });
    await Promise.all([loadAudits(), refreshNetworkDashboard()]);
    return contribution;
  }

  async function runAcceptedAuditPipeline(
    jobId: string,
    provider: string,
    model: string,
  ) {
    const contributions = await invoke<NodeContribution[]>("run_accepted_audit_pipeline", {
      jobId,
      provider,
      model,
    });
    await Promise.all([loadAudits(), refreshNetworkDashboard()]);
    return contributions;
  }

  async function verifyCampaignContribution(
    contributionId: string,
    decision = "accepted",
    reasonCode = "COVERAGE_ACCEPTED",
    reason = "Contribution is bounded, signed, and useful for campaign coverage.",
  ) {
    const credits = await invoke<CreditAllocation[]>("verify_campaign_contribution", {
      contributionId,
      decision,
      reasonCode,
      reason,
    });
    await refreshNetworkDashboard();
    return credits;
  }

  async function verifyNextPendingContribution() {
    if (!isTauriRuntime()) return null;
    const issued = await invoke<NetworkVerificationIssued | null>("verify_next_pending_contribution");
    if (issued) {
      await refreshNetworkDashboard();
    }
    return issued;
  }

  async function exportCampaignReport(campaignId: string) {
    return invoke<ExportedReportBundle>("export_campaign_report", { campaignId });
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

  async function routeAudit(jobId: string) {
    const job = await invoke<AuditJob>("route_audit", { jobId });
    await loadAudits();
    return job;
  }

  async function runAudit(jobId: string) {
    const job = await invoke<AuditJob>("run_audit", { jobId });
    await loadAudits();
    return job;
  }

  async function approveResult(jobId: string) {
    const job = await invoke<AuditJob>("approve_result", { jobId });
    await loadAudits();
    return job;
  }

  return {
    startNode,
    refreshPeers,
    refreshNetworkInfo,
    connectPeer,
    loadAudits,
    loadProtocolCampaigns,
    refreshNetworkDashboard,
    getCampaignSnapshot,
    listGuardianTargets,
    getGitHubAccessStatus,
    inspectGithubRepository,
    listLocalModelProviders,
    listLocalModelModels,
    refreshCreditSummary,
    migrateLegacyJobs,
    createAudit,
    createProtocolCampaign,
    claimCampaignWorkUnit,
    runClaimedWorkUnit,
    recordCampaignContribution,
    runCampaignAuditSkill,
    runCampaignAuditPipeline,
    runAcceptedAuditSkill,
    runAcceptedAuditPipeline,
    verifyCampaignContribution,
    verifyNextPendingContribution,
    exportCampaignReport,
    offerAudit,
    acceptOffer,
    routeAudit,
    runAudit,
    approveResult,
  };
}
