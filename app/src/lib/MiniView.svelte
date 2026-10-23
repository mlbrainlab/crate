<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  // Startup spike: a static copy of the Linux variant of design/canvas/MiniView.dc.html.
  // Real data and behavior arrive in M2.
  type Kind = "dir" | "doc" | "txt";
  const rows: { name: string; kind: Kind; ext: string; meta: string }[] = [
    { name: "design", kind: "dir", ext: "DIR", meta: "38 items" },
    { name: "docs", kind: "dir", ext: "DIR", meta: "12 items" },
    { name: "photos", kind: "dir", ext: "DIR", meta: "10 items" },
    { name: "src", kind: "dir", ext: "DIR", meta: "81 items" },
    { name: "brief.pdf", kind: "doc", ext: "PDF", meta: "3.2 MB" },
    { name: "README.md", kind: "txt", ext: "MD", meta: "4 KB" },
    { name: "LICENSE", kind: "txt", ext: "TXT", meta: "1 KB" },
  ];
</script>

<section class="mini" aria-label="Mini view">
  <header data-tauri-drag-region>
    <button class="ib" aria-label="Main menu">
      <svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="M4 6h16M4 12h16M4 18h16"></path></svg>
    </button>
    <span class="title" data-tauri-drag-region>project-assets.zip</span>
    <button class="close" aria-label="Close" onclick={() => getCurrentWindow().close()}>
      <svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 6l12 12M18 6 6 18"></path></svg>
    </button>
  </header>

  <div class="summary">
    <span class="badge">
      <svg class="i" viewBox="0 0 24 24" aria-hidden="true"
        ><path d="M21 8v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8"></path><rect x="2" y="3" width="20" height="5" rx="1"></rect><path d="M10 12h4"></path></svg
      >
    </span>
    <div class="info">
      <div class="name">project-assets.zip</div>
      <div class="stats">142 items · 2.4 GB → 812 MB</div>
      <div class="chips">
        <span class="chip zip">ZIP</span><span class="chip ok">Tested OK</span><span class="chip enc">1 encrypted</span>
      </div>
    </div>
    <button class="ib" aria-label="Open in full view" title="Open in full view (Ctrl+Enter)">
      <svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"></path></svg>
    </button>
  </div>

  <div class="search-wrap">
    <label class="search">
      <svg class="i" viewBox="0 0 24 24" aria-hidden="true"><circle cx="11" cy="11" r="7"></circle><path d="m20 20-3.5-3.5"></path></svg>
      <span class="sr">Search archive</span>
      <input type="text" placeholder="Search 142 items…" />
    </label>
  </div>

  <div class="rows">
    {#each rows as r (r.name)}
      <button class="row">
        <span class="tile {r.kind}">{r.ext}</span>
        <span class="row-name">{r.name}</span>
        <span class="meta">{r.meta}</span>
      </button>
    {/each}
  </div>

  <footer>
    <div class="split">
      <button class="primary">
        <svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="M12 3v12"></path><path d="m7 10 5 5 5-5"></path><path d="M5 21h14"></path></svg>
        Extract all to “project-assets/”
      </button>
      <button class="more" aria-label="More extract options">
        <svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="m6 9 6 6 6-6"></path></svg>
      </button>
    </div>
    <div class="dest">
      <svg class="i" viewBox="0 0 24 24" aria-hidden="true"
        ><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"></path></svg
      >
      ~/Downloads/project-assets/
      <a href="#extract-to">Extract to…</a>
    </div>
  </footer>
</section>

<style>
  .mini {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: var(--bg);
  }

  header {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 46px;
    padding: 0 10px;
    background: var(--chrome);
    border-bottom: 1px solid var(--line);
    flex: none;
  }

  .title {
    flex: 1;
    text-align: center;
    font-weight: 600;
  }

  .ib {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--ink-2);
    cursor: pointer;
  }

  .ib:hover {
    background: var(--surface-2);
  }

  .close {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    margin-right: 4px;
    border-radius: 50%;
    border: 0;
    padding: 0;
    background: color-mix(in srgb, var(--line) 60%, var(--chrome));
    color: var(--ink-2);
    cursor: pointer;
  }

  .close .i {
    width: 12px;
    height: 12px;
  }

  .summary {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px 18px 14px;
  }

  .badge {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 48px;
    height: 48px;
    border-radius: 13px;
    background: var(--accent);
    color: white;
  }

  .badge .i {
    width: 24px;
    height: 24px;
  }

  .info {
    flex: 1;
    min-width: 0;
  }

  .name {
    font-size: 15px;
    font-weight: 700;
  }

  .stats {
    margin-top: 3px;
    font-size: 12px;
    color: var(--ink-2);
  }

  .chips {
    display: flex;
    gap: 5px;
    margin-top: 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    height: 20px;
    padding: 0 7px;
    border-radius: 5px;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }

  .chip.zip {
    background: var(--accent-soft);
    color: var(--accent-soft-ink);
  }

  .chip.ok {
    background: color-mix(in srgb, var(--success) 12%, var(--bg));
    color: var(--success);
  }

  .chip.enc {
    background: color-mix(in srgb, var(--crypto) 12%, var(--bg));
    color: var(--crypto);
  }

  .search-wrap {
    padding: 0 18px 8px;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface-2);
  }

  .search .i {
    width: 15px;
    height: 15px;
    color: var(--ink-3);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    background: transparent;
    color: var(--ink);
    font: 400 13px var(--font-sans);
  }

  .sr {
    position: absolute;
    left: -9999px;
  }

  .rows {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: 2px 8px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 38px;
    flex: none;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    text-align: left;
  }

  .row:hover {
    background: var(--surface-2);
  }

  .tile {
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    font-size: 9px;
    font-weight: 700;
  }

  .tile.dir {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .tile.doc {
    background: color-mix(in srgb, var(--danger) 10%, var(--bg));
    color: var(--danger);
  }

  .tile.txt {
    background: var(--surface-2);
    color: var(--ink-2);
  }

  .row-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .meta {
    font-size: 12px;
    color: var(--ink-3);
  }

  footer {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 18px 16px;
    border-top: 1px solid var(--line);
    background: var(--chrome);
  }

  .split {
    display: flex;
  }

  .split button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    height: 42px;
    border: 0;
    color: white;
    font-weight: 600;
    cursor: pointer;
  }

  .primary {
    flex: 1;
    border-radius: 10px 0 0 10px;
    background: var(--accent);
  }

  .more {
    width: 42px;
    padding: 0;
    border-radius: 0 10px 10px 0;
    background: color-mix(in srgb, var(--accent) 82%, black);
  }

  .dest {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--ink-2);
  }

  .dest .i {
    width: 14px;
    height: 14px;
  }

  .dest a {
    margin-left: auto;
    font-weight: 500;
    color: var(--accent-soft-ink);
  }
</style>
