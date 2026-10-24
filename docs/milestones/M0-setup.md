# M0: Setup & skeleton

**Goal:** an empty Crate window opens on Ubuntu, built from the final repo layout.
**Done when:** `pnpm tauri dev` shows a window with Inter and the design colors, and CI is green.

## Decisions to make first
- [x] UI framework: **Svelte 5** (default) or React
- [x] License: MIT, Apache-2.0 or GPL-3.0. Add the `LICENSE` file. → **MIT**

## Tasks
### Machine (Ubuntu)
- [x] Install Rust via rustup (stable), Node LTS, pnpm
- [x] Install the Tauri Linux packages: `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev` (check the current list in the Tauri v2 prerequisites guide)

### Repo
- [x] Cargo workspace: `crates/crate-core` (lib), `crates/crate-cli` (bin `crate`), `app/src-tauri`
- [x] Scaffold `app/` with `create-tauri-app` (Vite + TS + chosen framework)
- [x] `app/src/styles/tokens.css`: every color variable from `design/DESIGN.md` (light + dark), Inter bundled locally (no Google Fonts at runtime)
- [x] **Startup spike:** a hello-world mini view (static HTML from `MiniView.dc.html`). Measure cold and warm time from launch to visible window on Ubuntu: log a timestamp at process start and at the first paint, and run it 10× (e.g. with `hyperfine`). Record the numbers in `docs/perf.md`
- [ ] If the spike misses the budget in ROADMAP even after tuning (show the window only after the first paint, keep the bundle small, no web fonts loaded over the network), build the same spike in Slint and compare before choosing
- [x] App icon: `pnpm tauri icon assets/icons/png/crate-1024.png` (see `assets/icons/README.md`)
- [x] Empty window with the standard header (title "Crate", the window controls from `Main.dc.html`)
- [x] `tests/fixtures/README.md` describing which archive files will live there
- [x] `.github/workflows/ci.yml`: on Ubuntu run `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, and the web build
- [x] `.github/FUNDING.yml` (placeholders), `README.md` stub, `CONTRIBUTING.md` stub

## Design references
- `design/canvas/Main.dc.html` (header, sidebar spacing)
- `design/DESIGN.md` → color tokens

## Acceptance (on Ubuntu)
- [x] `pnpm tauri dev` opens the window, the text is in Inter, the background matches the light tokens
- [x] Switching the system to dark mode switches the window colors
- [x] CI passes on a push
- [x] `docs/perf.md` shows cold/warm startup numbers, and the UI choice is recorded

## Prompt for Claude Code
> Read CLAUDE.md, docs/ROADMAP.md and docs/milestones/M0-setup.md. Set up the workspace and Tauri app exactly as described, using <Svelte 5|React>. Stop after the acceptance checks pass and tick the boxes.
