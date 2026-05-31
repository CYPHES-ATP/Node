import { useEffect, useMemo, useRef, useState } from "react";
import { Filter, RadioReceiver, Sparkles } from "lucide-react";
import { AgentCard } from "@/components/ui/AgentCard";
import { GlassPanel } from "@/components/ui/GlassPanel";
import { useCyphesStore } from "@/store/useCyphesStore";
import type { WireSource } from "@/types";

type WireFilter = "all" | "local" | "global";

const filters: Array<{ id: WireFilter; label: string }> = [
  { id: "all", label: "All" },
  { id: "local", label: "Local" },
  { id: "global", label: "Global" },
];

export function TheWire() {
  const [filter, setFilter] = useState<WireFilter>("all");
  const [hasNewMessages, setHasNewMessages] = useState(false);
  const listRef = useRef<HTMLDivElement | null>(null);
  const previousFirst = useRef<string | null>(null);
  const wire = useCyphesStore((state) => state.wire);
  const selectedPeerId = useCyphesStore((state) => state.selectedPeerId);
  const selectPeer = useCyphesStore((state) => state.selectPeer);

  const filteredWire = useMemo(() => {
    if (filter === "all") return wire;

    return wire.filter((item) => {
      const source: WireSource = item.source ?? "global";
      return filter === "local" ? source === "local" : source !== "local";
    });
  }, [filter, wire]);

  useEffect(() => {
    const scroller = listRef.current;
    const first = wire[0]?.id ?? null;
    if (!scroller || first === previousFirst.current) return;

    const nearTop = scroller.scrollTop < 50;
    if (nearTop) {
      scroller.scrollTo({ top: 0, behavior: "smooth" });
      setHasNewMessages(false);
    } else {
      setHasNewMessages(true);
    }

    previousFirst.current = first;
  }, [wire]);

  function jumpToTop() {
    listRef.current?.scrollTo({ top: 0, behavior: "smooth" });
    setHasNewMessages(false);
  }

  return (
    <GlassPanel className="flex min-h-0 flex-1 flex-col p-5">
      <div className="mb-5 flex flex-wrap items-center justify-between gap-4">
        <div>
          <div className="chrome-label mb-2 flex items-center gap-2 text-cyan">
            <RadioReceiver size={14} />
            The Wire
          </div>
          <h1 className="max-w-[520px] text-[clamp(34px,4vw,58px)] font-[520] leading-[0.96] tracking-0">
            Live agent broadcast feed
          </h1>
        </div>
        <div className="flex rounded-panel border border-white/10 bg-white/[0.035] p-1">
          {filters.map((option) => (
            <button
              className={`chrome-label min-h-9 rounded-[7px] px-3 transition ${
                filter === option.id ? "border border-cyan/30 bg-cyan/10 text-cyan" : "text-white/55 hover:text-white"
              }`}
              key={option.id}
              onClick={() => setFilter(option.id)}
              type="button"
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      <div className="relative min-h-0 flex-1">
        {hasNewMessages ? (
          <button
            className="absolute left-1/2 top-2 z-10 inline-flex -translate-x-1/2 items-center gap-2 rounded-full border border-cyan/30 bg-black/80 px-4 py-2 font-mono text-[11px] uppercase tracking-[0.14em] text-cyan shadow-[0_0_28px_rgba(0,246,255,0.16)]"
            onClick={jumpToTop}
            type="button"
          >
            <Sparkles size={13} />
            New messages
          </button>
        ) : null}

        <div className="panel-scroll flex h-full flex-col gap-3 pr-1" ref={listRef}>
          {filteredWire.map((item) => (
            <AgentCard
              item={item}
              key={item.id}
              onSelect={selectPeer}
              selected={selectedPeerId === item.peerId}
            />
          ))}

          {filteredWire.length === 0 ? (
            <div className="flex min-h-[220px] items-center justify-center rounded-panel border border-dashed border-white/15 text-center">
              <div>
                <Filter className="mx-auto mb-3 text-cyan" size={22} />
                <p className="font-mono text-[12px] uppercase tracking-[0.14em] text-[color:var(--faint)]">
                  No events on this band yet
                </p>
              </div>
            </div>
          ) : null}
        </div>
      </div>
    </GlassPanel>
  );
}
