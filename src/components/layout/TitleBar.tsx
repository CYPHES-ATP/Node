import { type MouseEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Minus, Square, X } from "lucide-react";
import { isTauriRuntime, truncatePeerId } from "@/lib/utils";
import { useCyphesStore } from "@/store/useCyphesStore";

export function TitleBar() {
  const nodeStatus = useCyphesStore((state) => state.nodeStatus);
  const peerId = useCyphesStore((state) => state.peerId);
  const usesNativeMacControls =
    isTauriRuntime() && /Macintosh|Mac OS X/.test(window.navigator.userAgent);

  async function windowAction(action: "minimize" | "maximize" | "close") {
    if (!isTauriRuntime()) return;
    const window = getCurrentWindow();
    if (action === "minimize") await window.minimize();
    if (action === "maximize") await window.toggleMaximize();
    if (action === "close") await window.close();
  }

  function startWindowDrag(event: MouseEvent<HTMLElement>) {
    if (!isTauriRuntime() || event.button !== 0) return;
    if ((event.target as HTMLElement).closest("button")) return;
    void getCurrentWindow().startDragging();
  }

  return (
    <header
      className={`titlebar${usesNativeMacControls ? " titlebar-native-mac" : ""}`}
      data-tauri-drag-region
      onMouseDown={startWindowDrag}
    >
      <div className="brand" data-tauri-drag-region>
        <img alt="" src="/cyphes-mark.svg" />
        <span>CYPHES</span>
      </div>

      <div className="titlebar-status" data-tauri-drag-region>
        <span className={`status-light status-${nodeStatus}`} />
        <span>{nodeStatus === "online" ? truncatePeerId(peerId, 6, 5) : nodeStatus}</span>
      </div>

      {usesNativeMacControls ? (
        <div aria-hidden="true" className="window-controls-spacer" data-tauri-drag-region />
      ) : (
        <div className="window-controls">
          <button aria-label="Minimize" onClick={() => void windowAction("minimize")} type="button">
            <Minus size={14} />
          </button>
          <button aria-label="Maximize" onClick={() => void windowAction("maximize")} type="button">
            <Square size={12} />
          </button>
          <button aria-label="Close" onClick={() => void windowAction("close")} type="button">
            <X size={14} />
          </button>
        </div>
      )}
    </header>
  );
}
