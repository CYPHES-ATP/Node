import { useMemo, useState } from "react";
import { Code2, FileText, Globe2, Network, Radar, Send, ShieldCheck } from "lucide-react";
import { CapabilityPill } from "@/components/ui/CapabilityPill";
import { GlassPanel } from "@/components/ui/GlassPanel";
import { Identicon } from "@/components/ui/Identicon";
import { StatusDot } from "@/components/ui/StatusDot";
import { TerminalRow } from "@/components/ui/TerminalRow";
import { useP2P } from "@/hooks/useP2P";
import { formatRelativeTime, toTitle, truncatePeerId } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";

const capabilityIcons = [
  { match: "web", icon: Globe2 },
  { match: "code", icon: Code2 },
  { match: "test", icon: ShieldCheck },
  { match: "file", icon: FileText },
  { match: "scan", icon: Radar },
];

export function AgentProfile() {
  const [sourceOpen, setSourceOpen] = useState(true);
  const selectedPeerId = useCyphesStore((state) => state.selectedPeerId);
  const peers = useCyphesStore((state) => state.peers);
  const forkCapabilities = useCyphesStore((state) => state.forkCapabilities);
  const { sendPing } = useP2P();

  const agent = selectedPeerId ? peers[selectedPeerId] : undefined;

  const atpDocument = useMemo(() => {
    if (!agent) return "{}";

    return JSON.stringify(
      {
        agent_id: agent.peerId,
        name: agent.name,
        endpoint: agent.endpoint ?? null,
        capabilities: agent.capabilities,
        source: agent.source,
        reputation: {
          attestations: agent.attestations,
          tasks_completed: agent.tasksCompleted,
        },
        last_seen: new Date(agent.lastSeen).toISOString(),
      },
      null,
      2,
    );
  }, [agent]);

  if (!agent) {
    return (
      <GlassPanel className="flex min-h-0 flex-1 items-center justify-center p-6">
        <div className="text-center">
          <Network className="mx-auto mb-4 text-cyan" size={28} />
          <p className="chrome-label text-[color:var(--faint)]">Select an agent on The Wire</p>
        </div>
      </GlassPanel>
    );
  }

  return (
    <GlassPanel className="panel-scroll min-h-0 flex-1 border-l-[1px] border-l-[color:var(--line-strong)] p-5">
      <div className="mb-6 flex items-start gap-4">
        <Identicon seed={agent.peerId} className="h-[92px] w-[92px] shrink-0" />
        <div className="min-w-0">
          <div className="chrome-label mb-2 text-cyan">Agent Profile</div>
          <h2 className="break-words text-[clamp(28px,3vw,42px)] font-[520] leading-none">{agent.name}</h2>
          <div className="mt-3 flex flex-wrap items-center gap-3 font-mono text-[11px] uppercase tracking-[0.12em] text-[color:var(--muted)]">
            <StatusDot status={agent.status} pulse={agent.status === "online"} label={agent.status} />
            {agent.location ? <span>{agent.location}</span> : null}
            <span>{formatRelativeTime(agent.lastSeen)}</span>
          </div>
        </div>
      </div>

      <div className="mb-5 rounded-panel border border-white/10 bg-black/20 px-4">
        <TerminalRow label="peer">{truncatePeerId(agent.peerId, 10, 8)}</TerminalRow>
        <TerminalRow label="endpoint">{agent.endpoint ?? "broadcast only"}</TerminalRow>
        <TerminalRow label="joined">{agent.joinedAt ? new Date(agent.joinedAt).toLocaleDateString() : "unknown"}</TerminalRow>
      </div>

      <div className="mb-5">
        <div className="chrome-label mb-3 text-[color:var(--faint)]">Capabilities</div>
        <div className="grid gap-2">
          {agent.capabilities.map((capability) => {
            const match = capabilityIcons.find((item) => capability.includes(item.match));
            const Icon = match?.icon ?? FileText;

            return (
              <div
                className="flex items-center gap-3 rounded-panel border border-white/10 bg-white/[0.035] p-3"
                key={capability}
              >
                <span className="inline-flex h-8 w-8 items-center justify-center rounded-full border border-cyan/20 bg-cyan/10 text-cyan">
                  <Icon size={15} />
                </span>
                <div className="min-w-0">
                  <div className="font-mono text-[12px] uppercase tracking-[0.12em] text-[color:var(--ink)]">
                    {toTitle(capability)}
                  </div>
                  <div className="mt-1 text-[12px] text-[color:var(--faint)]">Advertised in live capability card</div>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <div className="mb-5 grid grid-cols-2 gap-3">
        <div className="rounded-panel border border-white/10 bg-white/[0.035] p-4">
          <span className="chrome-label text-[color:var(--faint)]">Attestations</span>
          <strong className="mt-2 block text-3xl font-[520] text-green">{agent.attestations}</strong>
        </div>
        <div className="rounded-panel border border-white/10 bg-white/[0.035] p-4">
          <span className="chrome-label text-[color:var(--faint)]">Tasks Done</span>
          <strong className="mt-2 block text-3xl font-[520] text-cyan">{agent.tasksCompleted}</strong>
        </div>
      </div>

      <div className="mb-5">
        <button
          className="chrome-label mb-2 flex w-full items-center justify-between rounded-panel border border-white/10 bg-white/[0.035] px-3 py-2 text-left text-cyan"
          onClick={() => setSourceOpen((open) => !open)}
          type="button"
        >
          ATP document viewer
          <span>{sourceOpen ? "hide" : "view"}</span>
        </button>
        {sourceOpen ? (
          <pre className="max-h-[240px] overflow-auto rounded-panel border border-cyan/15 bg-black/40 p-4 font-mono text-[11px] leading-5 text-[color:var(--muted)]">
            {atpDocument}
          </pre>
        ) : null}
      </div>

      <div className="grid grid-cols-2 gap-3">
        <button className="btn-primary min-w-0 px-4" onClick={() => void sendPing(agent.peerId, agent.name)} type="button">
          <Send size={15} />
          Ping
        </button>
        <button className="btn-secondary min-w-0 px-4" onClick={() => forkCapabilities(agent.capabilities)} type="button">
          <CapabilityPill value="fork" />
          Config
        </button>
      </div>
    </GlassPanel>
  );
}
