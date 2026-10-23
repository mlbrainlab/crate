<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  let { title = "Crate" }: { title?: string } = $props();

  // Resolved on click, so the UI also renders outside Tauri (plain Vite in a browser).
  const win = () => getCurrentWindow();
</script>

<!-- Linux header bar (Main.dc.html). Windows and macOS use the native title bar (M7/M8). -->
<header data-tauri-drag-region>
  <div class="title" data-tauri-drag-region>{title}</div>
  <div class="controls">
    <button class="wc" aria-label="Minimize" onclick={() => win().minimize()}>
      <svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="M5 12h14"></path></svg>
    </button>
    <button class="wc" aria-label="Maximize" onclick={() => win().toggleMaximize()}>
      <svg class="i" viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="1"></rect></svg>
    </button>
    <button class="wc" aria-label="Close" onclick={() => win().close()}>
      <svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18"></path></svg>
    </button>
  </div>
</header>

<style>
  header {
    position: relative;
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 56px;
    padding: 0 12px;
    border-bottom: 1px solid var(--line);
    background: var(--chrome);
    box-sizing: border-box;
  }

  .title {
    flex: 1;
    text-align: center;
    font-size: 14px;
    font-weight: 600;
  }

  .controls {
    position: absolute;
    right: 12px;
    display: flex;
    gap: 8px;
  }

  .wc {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 0;
    padding: 0;
    background: color-mix(in srgb, var(--line) 60%, var(--chrome));
    color: var(--ink-2);
    cursor: pointer;
  }

  .wc:hover {
    background: var(--line);
    color: var(--ink);
  }

  .wc .i {
    width: 12px;
    height: 12px;
  }
</style>
