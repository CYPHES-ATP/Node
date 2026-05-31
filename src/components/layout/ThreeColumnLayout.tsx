import { AgentProfile } from "@/components/panels/AgentProfile";
import { MyStation } from "@/components/panels/MyStation";
import { TheWire } from "@/components/panels/TheWire";

export function ThreeColumnLayout() {
  return (
    <main className="command-grid">
      <section className="panel-stack panel-scroll min-w-0 pr-1" aria-label="My Station">
        <MyStation />
      </section>
      <section className="panel-stack min-w-0" aria-label="The Wire">
        <TheWire />
      </section>
      <section className="panel-stack profile-column min-w-0" aria-label="Agent Profile">
        <AgentProfile />
      </section>
    </main>
  );
}
