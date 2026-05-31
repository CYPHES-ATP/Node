import { useState } from "react";
import { Edit3, RadioTower, Radar, ToggleLeft, ToggleRight } from "lucide-react";
import { CapabilityPill } from "@/components/ui/CapabilityPill";
import { GlassPanel } from "@/components/ui/GlassPanel";
import { Identicon } from "@/components/ui/Identicon";
import { StatusDot } from "@/components/ui/StatusDot";
import { TerminalRow } from "@/components/ui/TerminalRow";
import { NetworkMap } from "@/components/panels/NetworkMap";
import { useP2P } from "@/hooks/useP2P";
import { truncatePeerId } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";

export function MyStation() {
  const [broadcasting, setBroadcasting] = useState(false);
  const myAgent = useCyphesStore((state) => state.myAgent);
  const networkStats = useCyphesStore((state) => state.networkStats);
  const setMyAgent = useCyphesStore((state) => state.setMyAgent);
  const setToast = useCyphesStore((state) => state.setToast);
  const { broadcastAdvertise, refreshPeers } = useP2P();

  async function handleBeacon() {
    setBroadcasting(true);
    try {
      await broadcastAdvertise();
    } finally {
      window.setTimeout(() => setBroadcasting(false), 1400);
    }
  }

  async function handleScan() {
    await refreshPeers();
    setToast("Discovery scan refreshed from the local swarm.");
  }

  return (
    <>
      <GlassPanel strong className="p-5">
        <div className="mb-5 flex items-start gap-4">
          <Identicon seed={myAgent.peerId || myAgent.name} className="h-[74px] w-[74px] shrink-0" />
          <div className="min-w-0 flex-1">
            <label className="chrome-label mb-2 flex items-center gap-2 text-cyan" htmlFor="agent-name">
              <Edit3 size={13} />
              Agent identity
            </label>
            <input
              id="agent-name"
              className="agent-name-input"
              value={myAgent.name}
              onChange={(event) => setMyAgent({ name: event.currentTarget.value.toUpperCase() })}
            />
            <p className="mt-2 font-mono text-[11px] text-[color:var(--faint)]">{truncatePeerId(myAgent.peerId)}</p>
          </div>
        </div>

        <div className="mb-5 flex items-center justify-between rounded-panel border border-white/10 bg-white/[0.035] px-3 py-2">
          <span className="chrome-label flex items-center gap-2 text-[color:var(--muted)]">
            <StatusDot status={myAgent.isOnline ? "online" : "offline"} pulse={myAgent.isOnline} />
            {myAgent.isOnline ? "Online" : "Offline"}
          </span>
          <button
            aria-label="Toggle online status"
            className="text-cyan transition hover:text-green"
            onClick={() => setMyAgent({ isOnline: !myAgent.isOnline })}
            type="button"
          >
            {myAgent.isOnline ? <ToggleRight size={32} /> : <ToggleLeft size={32} />}
          </button>
        </div>

        <div className="mb-5">
          <div className="chrome-label mb-3 text-[color:var(--faint)]">Capabilities</div>
          <div className="flex flex-wrap gap-2">
            {myAgent.capabilities.map((capability) => (
              <CapabilityPill key={capability} value={capability} />
            ))}
          </div>
        </div>

        <div className="station-terminal rounded-panel border border-white/10 bg-black/20 px-4">
          <TerminalRow label="bridge">
            <span className={myAgent.openClawConnected ? "terminal-good" : "text-orange"}>
              {myAgent.openClawConnected ? "OpenClaw detected" : "OpenClaw missing"}
            </span>
          </TerminalRow>
          <TerminalRow label="endpoint">
            {myAgent.openClawConnected ? "localhost:8080/health" : "Start runtime"}
          </TerminalRow>
          <TerminalRow label="wire">{networkStats.messages} feed events cached</TerminalRow>
        </div>

        <div className="mt-5 grid grid-cols-2 gap-3">
          <button className="btn-primary min-w-0 px-4" disabled={broadcasting} onClick={() => void handleBeacon()} type="button">
            <RadioTower size={15} />
            {broadcasting ? "Broadcasting" : "Beacon"}
          </button>
          <button className="btn-secondary min-w-0 px-4" onClick={() => void handleScan()} type="button">
            <Radar size={15} />
            Scan
          </button>
        </div>
      </GlassPanel>

      <GlassPanel className="min-h-0 flex-1 p-5">
        <div className="mb-4 flex items-center justify-between">
          <h2 className="chrome-label text-cyan">Mesh Map</h2>
          <span className="font-mono text-[11px] text-[color:var(--faint)]">
            {networkStats.localPeers + networkStats.globalPeers} nodes
          </span>
        </div>
        <NetworkMap />
      </GlassPanel>
    </>
  );
}
