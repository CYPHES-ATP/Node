import { Activity, Globe2, Hand, RadioTower } from "lucide-react";
import { CapabilityPill } from "@/components/ui/CapabilityPill";
import { StatusDot } from "@/components/ui/StatusDot";
import { formatRelativeTime, toTitle } from "@/lib/utils";
import type { AgentMessage } from "@/types";

interface AgentCardProps {
  item: AgentMessage;
  selected?: boolean;
  onSelect: (peerId: string) => void;
}

export function AgentCard({ item, selected, onSelect }: AgentCardProps) {
  const Icon = item.msgType === "heartbeat" ? Activity : item.msgType === "ping" ? Hand : RadioTower;
  const isHandshake = item.msgType === "ping" || item.msgType === "pong";

  return (
    <button
      className={`wire-card w-full animate-wire-in p-4 text-left ${selected ? "animate-flash border-cyan/60" : ""}`}
      onClick={() => onSelect(item.peerId)}
      type="button"
    >
      <div className="mb-3 flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="chrome-label flex items-center gap-2 text-[color:var(--ink)]">
            <StatusDot status="online" />
            <span className="truncate">{item.name}</span>
          </div>
          <div className="mt-2 flex flex-wrap items-center gap-2 font-mono text-[11px] text-[color:var(--faint)]">
            {item.location ? <span>{item.location}</span> : null}
            <span>{item.capabilities.length} capabilities</span>
            <span>{formatRelativeTime(item.timestamp)}</span>
          </div>
        </div>
        <span className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-full border border-white/10 bg-white/[0.045] text-cyan">
          {item.source === "global" ? <Globe2 size={15} /> : <Icon size={15} />}
        </span>
      </div>

      {item.msgType === "heartbeat" ? (
        <p className="font-mono text-[12px] leading-6 text-[color:var(--muted)]">
          Reputation: <span className="terminal-good">{item.attestations ?? 0} attestations</span>
          {item.payload ? `, ${item.payload}` : ""}
        </p>
      ) : null}

      {isHandshake ? (
        <div className="space-y-3">
          <p className="font-mono text-[12px] leading-6 text-[color:var(--muted)]">
            {item.msgType === "pong" ? "PONG" : "PING"}: {item.payload ?? "hello"}
          </p>
          <div className="flex flex-wrap gap-2">
            <span className="capability-pill border-green/35 text-green">accept</span>
            <span className="capability-pill border-white/15 text-white/55">ignore</span>
          </div>
        </div>
      ) : null}

      {item.msgType === "advertise" ? (
        <div className="space-y-3">
          <div className="flex flex-wrap gap-2">
            {item.capabilities.slice(0, 4).map((capability) => (
              <CapabilityPill key={capability} value={capability} />
            ))}
          </div>
          <p className="text-sm leading-6 text-[color:var(--muted)]">
            {item.payload ?? `${toTitle(item.name)} is broadcasting availability.`}
          </p>
        </div>
      ) : null}
    </button>
  );
}
