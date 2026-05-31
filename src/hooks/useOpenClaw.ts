import { useEffect } from "react";
import { useCyphesStore } from "@/store/useCyphesStore";

interface OpenClawHealth {
  agent_id?: string;
  peer_id?: string;
  name?: string;
  capabilities?: string[];
  status?: string;
}

export function useOpenClaw() {
  const setOpenClaw = useCyphesStore((state) => state.setOpenClaw);
  const setMyAgent = useCyphesStore((state) => state.setMyAgent);

  useEffect(() => {
    let cancelled = false;

    async function check() {
      try {
        const response = await fetch("http://localhost:8080/health", {
          cache: "no-store",
          signal: AbortSignal.timeout(1400),
        });

        if (!response.ok) {
          throw new Error(`OpenClaw health returned ${response.status}`);
        }

        const data = (await response.json()) as OpenClawHealth;
        if (cancelled) return;

        setOpenClaw(true, data.capabilities);
        setMyAgent({
          name: data.name ?? "OPENCLAW_LOCAL",
          peerId: data.peer_id ?? data.agent_id ?? undefined,
          isOnline: data.status ? data.status.toLowerCase() === "online" : true,
        });
      } catch {
        if (!cancelled) {
          setOpenClaw(false);
        }
      }
    }

    check();
    const interval = window.setInterval(check, 12_000);

    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [setMyAgent, setOpenClaw]);
}
