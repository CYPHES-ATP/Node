import { useMemo } from "react";
import { useCyphesStore } from "@/store/useCyphesStore";

const positions = [
  { left: "6%", top: "18%" },
  { left: "62%", top: "8%" },
  { left: "70%", top: "64%" },
  { left: "8%", top: "70%" },
  { left: "36%", top: "80%" },
  { left: "76%", top: "35%" },
];

export function NetworkMap() {
  const peers = useCyphesStore((state) => Object.values(state.peers));

  const visiblePeers = useMemo(() => peers.slice(0, 6), [peers]);

  return (
    <div className="node-map">
      <svg className="absolute inset-0 h-full w-full" aria-hidden="true">
        {visiblePeers.map((peer, index) => {
          const position = positions[index % positions.length];
          const x2 = Number.parseFloat(position.left) + 12;
          const y2 = Number.parseFloat(position.top) + 8;

          return (
            <line
              key={peer.peerId}
              x1="50%"
              y1="50%"
              x2={`${x2}%`}
              y2={`${y2}%`}
              stroke="rgba(0,246,255,0.2)"
              strokeDasharray="4 8"
              className="animate-dash"
            />
          );
        })}
      </svg>

      <div className="node-core">YOU</div>
      {visiblePeers.map((peer, index) => (
        <div
          className="node animate-float"
          key={peer.peerId}
          style={{
            ...positions[index % positions.length],
            animationDelay: `${index * 180}ms`,
          }}
          title={peer.name}
        >
          {peer.name.replace("AGENT_", "A_").slice(0, 11)}
        </div>
      ))}
    </div>
  );
}
