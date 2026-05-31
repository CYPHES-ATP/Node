import { getCurrentWindow } from "@tauri-apps/api/window";
import { Menu, Minus, Square, X } from "lucide-react";
import { GlassPanel } from "@/components/ui/GlassPanel";
import { StatusDot } from "@/components/ui/StatusDot";
import { isTauriRuntime } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";

export function TitleBar() {
  const isOnline = useCyphesStore((state) => state.myAgent.isOnline);
  const networkStats = useCyphesStore((state) => state.networkStats);

  async function windowAction(action: "minimize" | "maximize" | "close") {
    if (!isTauriRuntime()) return;

    const window = getCurrentWindow();
    if (action === "minimize") await window.minimize();
    if (action === "maximize") await window.toggleMaximize();
    if (action === "close") await window.close();
  }

  return (
    <GlassPanel className="titlebar" data-tauri-drag-region>
      <div className="flex items-center gap-4" data-tauri-drag-region>
        <div className="flex items-baseline gap-3" data-tauri-drag-region>
          <span className="text-[18px] font-[760] tracking-0">CYPHES</span>
          <span className="font-mono text-[11px] uppercase tracking-[0.16em] text-[color:var(--faint)]">v0.1</span>
        </div>
        <div className="hidden items-center gap-3 font-mono text-[11px] uppercase tracking-[0.14em] text-[color:var(--muted)] sm:flex">
          <StatusDot status={isOnline ? "online" : "offline"} pulse={isOnline} label={isOnline ? "Online" : "Offline"} />
          <span>{networkStats.localPeers} local</span>
          <span>{networkStats.globalPeers} global</span>
        </div>
      </div>

      <div className="flex items-center gap-1">
        <button
          aria-label="Menu"
          className="inline-flex h-9 w-9 items-center justify-center rounded-[7px] border border-white/10 bg-white/[0.035] text-white/70 transition hover:border-cyan/30 hover:text-cyan"
          type="button"
        >
          <Menu size={15} />
        </button>
        <button
          aria-label="Minimize"
          className="inline-flex h-9 w-9 items-center justify-center rounded-[7px] border border-white/10 bg-white/[0.035] text-white/70 transition hover:border-cyan/30 hover:text-cyan"
          onClick={() => void windowAction("minimize")}
          type="button"
        >
          <Minus size={15} />
        </button>
        <button
          aria-label="Maximize"
          className="inline-flex h-9 w-9 items-center justify-center rounded-[7px] border border-white/10 bg-white/[0.035] text-white/70 transition hover:border-cyan/30 hover:text-cyan"
          onClick={() => void windowAction("maximize")}
          type="button"
        >
          <Square size={13} />
        </button>
        <button
          aria-label="Close"
          className="inline-flex h-9 w-9 items-center justify-center rounded-[7px] border border-white/10 bg-white/[0.035] text-white/70 transition hover:border-orange/50 hover:text-orange"
          onClick={() => void windowAction("close")}
          type="button"
        >
          <X size={15} />
        </button>
      </div>
    </GlassPanel>
  );
}
