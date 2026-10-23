import { invoke } from "@tauri-apps/api/core";

/**
 * Startup handshake with Rust (docs/perf.md). Call right after mounting the UI:
 * Rust shows the hidden window, then the next frame reports that it is visible.
 * WebKitGTK delivers no animation frames to a hidden window, so "ready" means
 * "DOM mounted", and only the post-show frame waits for requestAnimationFrame
 * (with a timer fallback in case no frame ever comes).
 */
export function signalReady(): void {
  if (!("__TAURI_INTERNALS__" in window)) return;
  void invoke("app_ready").then(() => {
    let done = false;
    const report = (signal: string) => {
      if (!done) {
        done = true;
        void invoke("app_visible", { signal });
      }
    };
    requestAnimationFrame(() => report("raf"));
    setTimeout(() => report("timeout"), 100);
  });
}
