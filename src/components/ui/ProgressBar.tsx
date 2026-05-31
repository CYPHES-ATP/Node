import type { CSSProperties } from "react";

interface ProgressBarProps {
  value: number;
}

export function ProgressBar({ value }: ProgressBarProps) {
  return (
    <div className="top-progress" aria-hidden="true">
      <span style={{ "--sync": `${Math.max(6, Math.min(100, value))}%` } as CSSProperties} />
    </div>
  );
}
