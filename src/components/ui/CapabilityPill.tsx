import { cn } from "@/lib/utils";

interface CapabilityPillProps {
  value: string;
  muted?: boolean;
}

export function CapabilityPill({ value, muted }: CapabilityPillProps) {
  return <span className={cn("capability-pill", muted && "opacity-60")}>{value}</span>;
}
