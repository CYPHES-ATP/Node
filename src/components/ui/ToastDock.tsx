import { useEffect } from "react";
import { Zap } from "lucide-react";
import { useCyphesStore } from "@/store/useCyphesStore";

export function ToastDock() {
  const toast = useCyphesStore((state) => state.toast);
  const setToast = useCyphesStore((state) => state.setToast);

  useEffect(() => {
    if (!toast) return undefined;

    const timer = window.setTimeout(() => setToast(null), 3200);
    return () => window.clearTimeout(timer);
  }, [setToast, toast]);

  if (!toast) return null;

  return (
    <div className="fixed bottom-5 left-1/2 z-50 flex -translate-x-1/2 items-center gap-3 rounded-full border border-cyan/30 bg-black/80 px-5 py-3 font-mono text-[12px] uppercase tracking-[0.12em] text-cyan shadow-[0_0_36px_rgba(0,246,255,0.18)] backdrop-blur-xl">
      <Zap size={14} />
      {toast}
    </div>
  );
}
