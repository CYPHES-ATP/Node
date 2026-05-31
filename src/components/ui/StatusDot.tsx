import { cn } from "@/lib/utils";
import type { AgentStatus } from "@/types";

interface StatusDotProps {
  status?: AgentStatus | "online" | "offline";
  pulse?: boolean;
  label?: string;
}

export function StatusDot({ status = "unknown", pulse, label }: StatusDotProps) {
  const isOnline = status === "online";

  return (
    <span className="inline-flex items-center gap-2">
      <span className="relative inline-flex h-2.5 w-2.5 items-center justify-center">
        {pulse && isOnline ? (
          <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-green opacity-75" />
        ) : null}
        <span
          className={cn(
            "relative inline-flex h-2.5 w-2.5 rounded-full",
            isOnline
              ? "bg-green shadow-[0_0_18px_rgba(199,255,71,0.65)]"
              : status === "offline"
                ? "bg-orange shadow-[0_0_16px_rgba(255,112,67,0.36)]"
                : "bg-white/35",
          )}
        />
      </span>
      {label ? <span>{label}</span> : null}
    </span>
  );
}
