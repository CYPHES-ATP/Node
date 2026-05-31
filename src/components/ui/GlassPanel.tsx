import type { HTMLAttributes, MouseEvent } from "react";
import { cn } from "@/lib/utils";

interface GlassPanelProps extends HTMLAttributes<HTMLDivElement> {
  strong?: boolean;
}

export function GlassPanel({ className, strong, onMouseMove, ...props }: GlassPanelProps) {
  function handleMouseMove(event: MouseEvent<HTMLDivElement>) {
    const rect = event.currentTarget.getBoundingClientRect();
    const mx = (event.clientX - rect.left) / rect.width;
    const my = (event.clientY - rect.top) / rect.height;

    event.currentTarget.style.setProperty("--mx", mx.toFixed(3));
    event.currentTarget.style.setProperty("--my", my.toFixed(3));
    onMouseMove?.(event);
  }

  return (
    <div
      className={cn("glass panel", strong && "glass-strong", className)}
      onMouseMove={handleMouseMove}
      {...props}
    />
  );
}
