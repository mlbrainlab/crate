# Startup performance

Budget (ROADMAP): mini view visible **≤ 500 ms after a cold double-click, ≤ 250 ms warm**, on Ubuntu.

## UI choice
**Tauri 2 + Svelte 5** (TypeScript, plain Vite, no SvelteKit), decided in M0. The Tauri spike misses the budget; the same spike in Slint meets it with room to spare (see *Tauri vs Slint* and *Verdict*). Whether to switch the UI is open, and must be settled before M2.

## Machine
| | |
|---|---|
| CPU / RAM | Intel Core i9-14900K (32 threads) / 125 GiB |
| GPU | NVIDIA GeForce GTX 1070 Ti |
| OS | Ubuntu 26.04.1 LTS, kernel 7.0.0-38, GNOME Shell 50.1 on Wayland |
| WebView | WebKitGTK 2.52.6 (`libwebkit2gtk-4.1`) |
| Versions | Tauri 2.12.5, Svelte 5.57.2, Vite 8.3.5, Slint 1.18.1, rustc 1.97.1 |

## Method
- Release build: `pnpm tauri build --no-bundle` (LTO, `codegen-units = 1`, `panic = "abort"`, stripped). Binary 4.7 MB; web bundle 312 KB, of which 270 KB is the Inter woff2 files (only the faces in use load).
- `target/release/crate-app --mini --startup-probe` opens the static mini view (520 × 620), logs its timeline to stderr and exits once the window is visible.
- Timeline points, measured from exec:
  - **pre-main**: process age at the first line of `main`, read from `/proc/self/stat` (10 ms resolution). Covers exec, dynamic loading of WebKitGTK/GTK and static initializers.
  - **window built**: the hidden window and webview exist (`setup()`).
  - **UI mounted**: Svelte has mounted and called `app_ready`; Rust shows the window.
  - **visible frame**: the first `requestAnimationFrame` after `show()`, the first frame the user can see.
  - **total** = pre-main + visible frame.
- `scripts/startup-bench.sh`: hyperfine, 10 runs each. **Warm**: 3 warmup runs first. **Cold**: `sync; echo 3 > /proc/sys/vm/drop_caches` before every run. Pages that the running desktop already maps (GTK, GLib…) stay cached, which is also true for a real user.
- hyperfine's wall time also includes process exit, so it is an upper bound.

## Tauri results (2026-10-23, 10 runs each, ms)

| | Warm median | min–max | Cold median | min–max |
|---|---:|---:|---:|---:|
| exec → main (pre-main) | 20 | 10–30 | 175 | 140–200 |
| window built | 76 | 71–97 | 181 | 164–209 |
| UI mounted | 150 | 142–183 | 290 | 271–323 |
| visible frame (from `main`) | 250 | 242–283 | 403 | 391–445 |
| **exec → visible (total)** | **267** | 252–303 | **576** | 536–635 |
| hyperfine exec → exit | 274 | 264–308 | 616 | 572–678 |
| **Budget** | **250** | | **500** | |

## Tauri vs Slint (2026-10-24)
The same static mini view (layout, copy, DESIGN.md colors, light/dark) built in Slint: `spikes/slint-mini/`, its own Cargo workspace with the same release profile. Slint's default features (winit backend on Wayland; FemtoVG and Skia renderers compiled in). "Visible" is Slint's first `AfterRendering` callback, the counterpart of the first animation frame after `show()` in Tauri. The text uses the system-installed Inter (a real port would embed the TTF). Benchmark: `BIN=spikes/slint-mini/target/release/slint-mini OUT=target/startup-bench-slint scripts/startup-bench.sh`.

| Median, ms (10 runs) | Tauri warm | Slint warm | Tauri cold | Slint cold |
|---|---:|---:|---:|---:|
| exec → main (pre-main) | 20 | 0 | 175 | 30 |
| window built | 76 | 23 | 181 | 99 |
| UI ready (mounted / component created) | 150 | 23 | 290 | 99 |
| visible frame (from `main`) | 250 | 92 | 403 | 252 |
| **exec → visible (total)** | **267** | **93** | **576** | **282** |
| hyperfine exec → exit | 274 | 101 | 616 | 322 |
| **Budget** | **250** | **250** | **500** | **500** |

Slint ranges: warm total 87–110, cold total 228–306.

| | Tauri + Svelte | Slint |
|---|---|---|
| Meets the budget | no (warm +17, cold +76 ms) | yes (warm −157, cold −218 ms) |
| Release binary | 4.7 MB + 312 KB web bundle | 14.3 MB (UI compiled in) |
| Runtime dependency on Linux | WebKitGTK (system package) | none beyond GL/Wayland/X11 libs |
| Reuse of the HTML/CSS mockups | direct | rewrite each screen in `.slint` |
| Glass theme (blur) | CSS `backdrop-filter` | no CSS-style backdrop blur built in as far as I know (verify before deciding); likely needs OS window effects or a workaround |
| RTL / Arabic text shaping | WebKit, mature | supported, less proven |

## Tuning tried (Tauri)
| Change | Effect (warm, visible) | Kept? |
|---|---|---|
| Window hidden until the UI is mounted, then `show()` | baseline | yes |
| Show the window immediately, let the webview paint into it | ~830 ms (much worse: WebKit loads slower into a mapped window) | no |
| Wait for `requestAnimationFrame` before `app_ready` | never fires while hidden (WebKitGTK), so it fell back to a 100 ms timer | no, `ready` fires on mount |
| `WEBKIT_DISABLE_DMABUF_RENDERER=1` | −25 to −35 ms | not yet: changes the GPU path for everything |
| `WEBKIT_DISABLE_COMPOSITING_MODE=1` | −25 to −35 ms | not yet: would break blur / the Glass theme |
| `GDK_BACKEND=x11` (XWayland) | +40 to +70 ms | no |
| Fonts bundled locally, `font-display: swap`, small bundle | already in place | yes |

## Where the time goes
- **Cold, before `main` (175 ms):** loading WebKitGTK and its dependencies from disk. No web-UI change can reduce it.
- **Window + webview (≈ 55 ms warm):** GTK window and WebKit web process start.
- **Page load → mount (≈ 75 ms warm, 110 ms cold):** WebKit loads `index.html`, CSS and 55 KB of JS.
- **`show()` → first visible frame (≈ 100 ms):** map the window and composite the first frame on Wayland.

## Verdict
- **Tauri + Svelte misses both budgets**: warm by ~17 ms, cold by ~76 ms. About 175 ms of the cold time is loading WebKitGTK before `main`, which no web-side tuning can remove.
- **Slint meets both with a wide margin**: warm 93 ms, cold 282 ms, roughly 3× and 2× faster.
- ROADMAP rule: *if Tauri can't hit the budget, switch the UI to Slint before M2*. The numbers trigger that rule. The trade-offs (mockups no longer copy over directly, no CSS blur for the Glass theme) are the open question for the decision.
- Other ways to keep Tauri, measured or estimated above: the two WebKit env settings (≈ −30 ms; warm would pass, cold would stay ~545 ms), or a resident single-instance process so later double-clicks skip process start (helps every launch after the first, not the first cold one).
