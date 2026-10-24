# Crate: build roadmap

Linux (Ubuntu) first. Windows and macOS come after a working Linux release.
Every milestone ends with something you can **run and click on Ubuntu**. No milestone is "just plumbing".

Design source of truth: `design/` (canvas boards in `design/canvas/*.dc.html`, rules in `design/DESIGN.md`, open items in `design/BACKLOG.md`).

---

## Stack (decided unless marked)

| Layer | Choice | Why |
|---|---|---|
| UI toolkit | **Slint 1.18** (Rust + `.slint` markup; winit backend, FemtoVG renderer) | No web engine to start: the M0 spike showed the mini view in 93 ms warm / 282 ms cold, against 267 / 576 ms for Tauri 2 + Svelte (`docs/perf.md`). Runs on Linux/Windows/macOS. Decided in M0 (Oct 24), replacing Tauri 2 + Svelte 5 |
| Archive engine | `crate-core` Rust library | Shared by the app and the CLI; easy to test |
| Formats | `zip`, `tar`, `flate2`, `xz2`/`liblzma`, `zstd`, `bzip2`, `sevenz-rust2`, `unrar` (read only) | Pure-Rust where possible, so nothing native to bundle |
| Filename encodings | `encoding_rs` + `chardetng` | Fixes CP1256 / CP437 / Shift-JIS names |
| Startup budget | Target: mini view visible ≤ 500 ms after a cold double-click, ≤ 250 ms warm, on Ubuntu | Measured in M0 with `scripts/startup-bench.sh`: Tauri missed it, Slint meets it. Re-measure whenever the mini view gains real work (M2 onwards) |
| Animations | Free, short (150–250 ms), purposeful; respect "reduce motion"; never delay the first frame. Extra motion that comes with the Glass theme is Pro | |
| Build | Cargo only; no Node toolchain | |
| License | **MIT** | Decided in M0 |
| Styling | Slint global `Tokens` (`app/ui/tokens.slint`) mirroring the color tokens in `design/DESIGN.md`; Inter compiled in from `assets/fonts/inter` | The HTML mockups stay the reference for layout, spacing and copy; each screen is rebuilt in `.slint` |
| Tests | `cargo test` with an archive test-file folder; a manual UI checklist per milestone | |

## Repository layout (target)

```
arxiv-app/
├─ CLAUDE.md               ← instructions for Claude Code
├─ design/                 ← mockups + design docs (exists)
├─ docs/
│  ├─ ROADMAP.md           ← this file
│  └─ milestones/M0…M9     ← one file per milestone
├─ crates/
│  ├─ crate-core/          ← archive engine (lib)
│  └─ crate-cli/           ← `crate` command-line tool
├─ app/                    ← Slint desktop app (src/ Rust, ui/ .slint files)
└─ tests/fixtures/         ← sample + malicious archives
```

## Milestones

| # | Milestone | You can do this on Ubuntu when it's done | Size |
|---|---|---|---|
| [M0](milestones/M0-setup.md) | Setup & skeleton | Run `cargo run -p crate-app` and see an empty Crate window in Inter with the design colors | S |
| [M1](milestones/M1-core-extract.md) | Core: list & extract safely | `crate l file.zip` and `crate x file.tar.zst` work from the terminal; malicious test archives are blocked | M |
| [M2](milestones/M2-mini-view.md) | Mini view | Double-click a .zip in Files → mini view opens → one click extracts with progress | M |
| [M3](milestones/M3-full-view.md) | Full view: browse | Browse folders inside an archive, select files, extract a selection, quick preview | L |
| [M4](milestones/M4-create-edit.md) | Create & edit | Compress a folder; add, rename and delete files inside a zip; test integrity | L |
| [M5](milestones/M5-linux-polish.md) | Linux polish | Operations panel, batch extract, settings, first run, dark mode, Arabic + encoding fix | L |
| [M6](milestones/M6-linux-release.md) | **Linux release v0.1** | Install from a GitHub Release (AppImage / .deb / .rpm); Flathub submission sent | M |
| [M7](milestones/M7-windows.md) | Windows port | Same features on Windows 11, unsigned installer + winget | M |
| [M8](milestones/M8-macos.md) | macOS port | Signed and notarized .dmg; Homebrew cask | M |
| [M9](milestones/M9-pro-glass.md) | Pro: Glass theme + icons | Unlock the Glass theme and alternate icons with a license key | M |

S ≈ a few evenings · M ≈ 1–2 weeks of evenings · L ≈ 2–3 weeks of evenings.

## Rules for every milestone

1. **Start by reading** the milestone file and the design boards it lists.
2. **Small vertical slices.** Each working session should end with something runnable and a git commit.
3. **Safety is never postponed.** The path-escape and symlink checks from M1 apply to every later feature that writes files.
4. **Don't jump ahead.** Features from later milestones go into `design/BACKLOG.md`, not into the code.
5. **Done means:** every acceptance check in the milestone file passes on Ubuntu, `cargo test` passes, and the milestone's checkboxes are ticked.

## Open decisions (from `design/BACKLOG.md`)

| Decision | Needed by |
|---|---|
| ~~UI framework~~: decided, Slint (replaced Tauri + Svelte 5 after the M0 startup spike) | done |
| ~~License~~: decided, MIT | done |
| ~~App icon~~: decided, concept #1 "Open Lid" (`assets/icons/`) | done |
| Passwords for tar.* (age) or ZIP/7z only | M4 |
| Pro model (honor-system key recommended) + payment provider | M9 |
