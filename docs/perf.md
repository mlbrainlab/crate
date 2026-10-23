# Startup performance

Budget (ROADMAP): mini view visible **≤ 500 ms after a cold double-click, ≤ 250 ms warm**, on Ubuntu.

## UI choice
**Tauri 2 + Svelte 5** (TypeScript, plain Vite, no SvelteKit). Decided in M0. The spike below misses the budget; the Slint comparison is pending (see *Verdict*).

## Machine
| | |
|---|---|
| CPU / RAM | Intel Core i9-14900K (32 threads) / 125 GiB |
| GPU | NVIDIA GeForce GTX 1070 Ti |
| OS | Ubuntu 26.04.1 LTS, kernel 7.0.0-38, GNOME Shell 50.1 on Wayland |
| WebView | WebKitGTK 2.52.6 (`libwebkit2gtk-4.1`) |
| Versions | Tauri 2.12.5, Svelte 5.57.2, Vite 8.3.5, rustc 1.97.1 |

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

## Results (2026-10-23, 10 runs each, ms)

| | Warm median | min–max | Cold median | min–max |
|---|---:|---:|---:|---:|
| exec → main (pre-main) | 20 | 10–30 | 175 | 140–200 |
| window built | 76 | 71–97 | 181 | 164–209 |
| UI mounted | 150 | 142–183 | 290 | 271–323 |
| visible frame (from `main`) | 250 | 242–283 | 403 | 391–445 |
| **exec → visible (total)** | **267** | 252–303 | **576** | 536–635 |
| hyperfine exec → exit | 274 | 264–308 | 616 | 572–678 |
| **Budget** | **250** | | **500** | |

## Tuning tried
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
Misses both budgets: **warm by ~17 ms, cold by ~76 ms**. Per ROADMAP, the next step is the same spike in Slint, compared on this machine before choosing. Options to weigh against Slint:
1. Accept the env-var tweaks above (≈ −30 ms: warm then meets the budget, cold ~545 ms).
2. Keep a small resident process (single instance) so a double-click only opens a window in an already running app: removes pre-main and WebKit start from every launch after the first.
3. Switch the mini view to Slint (pure Rust, no WebKit), as ROADMAP plans; `crate-core` is unaffected.
