import { makeIdenticon } from "@/lib/identicon";
import { cn } from "@/lib/utils";

interface IdenticonProps {
  seed: string;
  className?: string;
}

export function Identicon({ seed, className }: IdenticonProps) {
  const cells = makeIdenticon(seed);

  return (
    <div
      className={cn(
        "grid grid-cols-5 gap-[3px] rounded-panel border border-cyan/30 bg-black/30 p-2 shadow-[0_0_34px_rgba(0,246,255,0.09)]",
        className,
      )}
      aria-hidden="true"
    >
      {cells.map((cell) => (
        <span
          key={`${cell.x}-${cell.y}`}
          className={cn(
            "aspect-square rounded-[3px] border border-white/10",
            cell.active ? "opacity-95" : "opacity-10",
          )}
          style={{
            gridColumn: cell.x + 1,
            gridRow: cell.y + 1,
            background: cell.active ? cell.color : "rgba(255,255,255,0.08)",
            boxShadow: cell.active ? "0 0 16px rgba(0,246,255,0.18)" : undefined,
          }}
        />
      ))}
    </div>
  );
}
