import type { ReactNode } from "react";

interface TerminalRowProps {
  label: string;
  children: ReactNode;
}

export function TerminalRow({ label, children }: TerminalRowProps) {
  return (
    <div className="terminal-row">
      <span className="terminal-key">{label}</span>
      <span>{children}</span>
    </div>
  );
}
