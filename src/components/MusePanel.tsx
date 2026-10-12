import { useState } from "react";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

export default function MusePanel() {
  const [status, setStatus] = useState<string | null>(null);

  async function openMuse() {
    setStatus(null);
    try {
      const existing = await WebviewWindow.getByLabel("muse-chat");
      if (existing) {
        await existing.setFocus();
        return;
      }
      const w = new WebviewWindow("muse-chat", {
        url: "https://muse.ai",
        title: "Muse — your twin",
        width: 430,
        height: 860,
        center: true,
      });
      w.once("tauri://error", (e) => {
        setStatus(`Could not open Muse: ${String((e as { payload?: unknown }).payload ?? e)}`);
      });
    } catch (err) {
      setStatus(`Could not open Muse: ${String(err)}`);
    }
  }

  return (
    <div className="muse-panel">
      <h2>Ride with your twin</h2>
      <p>
        Muse lives here now. Open the chat, sign in once, and keep it beside
        you while you stream.
      </p>
      <button type="button" className="tab tab-active" onClick={openMuse}>
        Open Muse
      </button>
      {status && <p className="fix-hint">{status}</p>}
    </div>
  );
}
