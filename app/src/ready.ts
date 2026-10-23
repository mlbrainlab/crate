import { invoke } from "@tauri-apps/api/core";

/** Tell Rust the first frame is painted so it can show the (hidden) window. */
export function signalReady(): void {
  requestAnimationFrame(() => {
    void invoke("app_ready");
  });
}
