import { TitleBar } from "@/components/layout/TitleBar";
import { ThreeColumnLayout } from "@/components/layout/ThreeColumnLayout";
import { P2PProvider } from "@/components/providers/P2PProvider";
import { ProgressBar } from "@/components/ui/ProgressBar";
import { ToastDock } from "@/components/ui/ToastDock";
import { useOpenClaw } from "@/hooks/useOpenClaw";
import { useCyphesStore } from "@/store/useCyphesStore";

function App() {
  useOpenClaw();
  const sync = useCyphesStore((state) => state.networkStats.sync);

  return (
    <P2PProvider>
      <div className="app-shell">
        <ProgressBar value={sync} />
        <TitleBar />
        <ThreeColumnLayout />
        <ToastDock />
      </div>
    </P2PProvider>
  );
}

export default App;
