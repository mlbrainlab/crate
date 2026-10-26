# Startup performance

Budget (ROADMAP): mini view visible **≤ 500 ms after a cold double-click, ≤ 250 ms warm**, on Ubuntu.

## UI choice
**Slint**, decided on 2026-10-24. M0 first built the app with Tauri 2 + Svelte 5; that spike missed the budget, and the same spike in Slint met it with room to spare (see *Tauri vs Slint* and *Verdict*). The Tauri app was then replaced by the Slint app in `app/`. Its current numbers are under *Current app*.

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
The same static mini view (layout, copy, DESIGN.md colors, light/dark) built in Slint: `spikes/slint-mini/` (commit `78ac97d`; removed once `app/` became the Slint app), its own Cargo workspace with the same release profile. Slint's default features (winit backend on Wayland, FemtoVG OpenGL renderer). "Visible" is Slint's first `AfterRendering` callback, the counterpart of the first animation frame after `show()` in Tauri. The text uses the system-installed Inter (a real port would embed the TTF). Benchmark: `BIN=spikes/slint-mini/target/release/slint-mini OUT=target/startup-bench-slint scripts/startup-bench.sh` at that commit.

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
- ROADMAP rule: *if Tauri can't hit the budget, switch the UI to Slint before M2*. The numbers trigger that rule, and the UI moved to Slint on 2026-10-24, accepting the trade-offs (mockups no longer copy over directly, no in-window blur for the Glass theme).
- Other ways to keep Tauri, measured or estimated above: the two WebKit env settings (≈ −30 ms; warm would pass, cold would stay ~545 ms), or a resident single-instance process so later double-clicks skip process start (helps every launch after the first, not the first cold one).

## Current app (Slint, `app/`)
Same probe and script: `scripts/startup-bench.sh` builds `cargo build --release -p crate-app` and launches `target/release/crate-app --mini --startup-probe`. Differences from the spike: Inter 4.1 (400–700) compiled into the binary, the Wayland app id set through a winit hook. Release binary 17 MB.

| Median, ms (10 runs) | Warm | min–max | Cold | min–max |
|---|---:|---:|---:|---:|
| exec → main (pre-main) | 0 | 0–10 | 30 | 30–40 |
| window built | 24 | 18–31 | 121 | 88–158 |
| visible frame (from `main`) | 114 | 109–121 | 306 | 262–354 |
| **exec → visible (total)** | **114** | 109–125 | **336** | 302–384 |
| hyperfine exec → exit | 122 | 112–127 | 378 | 336–410 |
| **Budget** | **250** | | **500** | |

Measured 2026-10-24 on the machine above, after the header fixes. Both budgets are met: warm with 136 ms to spare, cold with 164 ms. Cold is ~54 ms slower than the spike (282 ms), most likely from reading the 1.6 MB of compiled-in Inter faces from disk; worth trimming (e.g. fewer faces or a subset) if M2's real work eats into the margin.

## Mini view with a real archive (M2)
Since M2 the probe opens an archive: `scripts/startup-bench.sh` generates `target/startup-bench/bench.zip` (2,000 deflated text files in nested folders, 1.3 MB) and runs `target/release/crate-app --startup-probe target/startup-bench/bench.zip`. The archive is read on a worker thread while the window is built; the clock stops at the first frame that shows the listing (`visible_ms`). Release binary 21 MB, up from 17 MB now that the app links the archive engine (UnRAR, liblzma, zstd, bzip2, 7z, zip).

| Median, ms (10 runs) | Warm | min–max | Cold | min–max |
|---|---:|---:|---:|---:|
| exec → main (pre-main) | 0 | 0–10 | 30 | 30–40 |
| window built | 23 | 21–32 | 81 | 68–120 |
| listing ready (worker) | 96 | 85–128 | 249 | 83–338 |
| **exec → listing visible (total)** | **111** | 93–137 | **292** | 241–379 |
| hyperfine exec → exit | 114 | 103–147 | 322 | 271–405 |
| **Budget** | **250** | | **500** | |

Measured 2026-10-24 on the machine above (the warm row repeats the earlier warm run within noise). Both budgets are met: warm with 139 ms to spare, cold with 208 ms. The listing is ready before the first frame in both modes, so reading the archive on a worker thread hides it completely; cold startup is even ~44 ms faster than the M0 static view (336 ms), likely run-to-run variation in cold disk reads.

After M3 (full view, Extract dialog, preview, grid), 2026-10-25: warm exec → listing visible **111 ms** median (82–133), unchanged; the listing itself is ready at 27 ms since big ZIPs no longer scan the whole file. Release binary 24 MB (+3 MB: XDG portal client for the folder picker, image decoders for preview and thumbnails, Trash support). None of these load before they are used.

After M4 (create, edit, properties, convert), 2026-10-25: warm exec → listing visible **133 ms** median (98–151), up from 111 ms, still within the 250 ms budget. Listing ready at 30 ms. Release binary 28 MB (+4 MB: age, the checksum crates, and the much larger archive window). Worth a cold re-run and a look at trimming before the M6 release.

After bundling "Crate Arabic" (IBM Plex Sans Arabic Medium + SemiBold, 477 KB), 2026-10-26: warm exec → listing visible **94.7 ms** median (85–113) vs **93.8 ms** (83–101) for the same build without it, measured back to back. First frame +8 ms. Release binary 27.1 MB (+0.5 MB). Cold not re-run (needs sudo).

After the Preferences screen (M5), 2026-10-26: warm exec → listing visible **102.2 ms** median (83–115), vs 94.7 ms before; the archive window now also reads the preferences and sets the theme. Within the 250 ms budget. Cold not re-run (needs sudo).

After translations and the right-to-left layout (M5), 2026-10-26: warm exec → listing visible **102.8 ms** median (93–120), unchanged. Release binary 31 MB.
