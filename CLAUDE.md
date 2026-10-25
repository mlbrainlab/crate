# Crate: notes for Claude Code

Crate is a cross-platform archiver (Rust, with a Slint UI). This is an open-source showcase project with a small optional Pro tier.

## Where things are
- Machines, builds and parallel sessions: `docs/WORKFLOW.md`.
- Build plan: `docs/ROADMAP.md`. Work on **one milestone at a time**: `docs/milestones/M*.md`.
- App icon: `assets/icons/` (concept #1 "Open Lid"; see its README). Font: `assets/fonts/inter/` (compiled in).
- UI: `app/ui/*.slint`, colors in `app/ui/tokens.slint`; platform code in `app/src/platform/`.
- Startup numbers and how to measure them: `docs/perf.md`, `scripts/startup-bench.sh`.
- Design: `design/DESIGN.md` (colors, rules, per-platform notes), `design/BACKLOG.md` (status of every item), mockups in `design/canvas/*.dc.html`. The mockups are plain HTML; open them in a browser or read them for exact spacing and copy.

## Working rules
- Linux (Ubuntu) first. Don't add Windows/macOS-specific code before M7/M8, except `cfg` stubs.
- Keep `crate-core` free of UI (Slint) dependencies. The app and the CLI both call it.
- Every file write during extraction goes through `crate_core::safety`: no path escapes, no symlinks pointing outside, decompression-bomb limits. Never bypass it.
- Never modify an original archive in place: write to a temp file in the same folder, then rename it over the original.
- Long operations run off the UI thread, report progress events, and can be cancelled.
- Use the color tokens from `design/DESIGN.md` (the `Tokens` global in `app/ui/tokens.slint`), not new hex values.
- Keep the mini view within the startup budget in the ROADMAP; re-run `scripts/startup-bench.sh` after changes to startup.
- Commit after each working slice. Tick the checkboxes in the milestone file as you go.
- If a task needs a decision listed under "Open decisions" in the ROADMAP, stop and ask.

## Commands
- `cargo run -p crate-app -- <archive>`: open an archive in the mini view (no argument: the full window)
- `cargo test -p crate-app --test mini_flow` / `--test full_flow`: drive the mini and full view headless and write screenshots to `target/ui-snapshots/`
- `cargo test --workspace`: all Rust tests
- `cargo run -p crate-cli -- x <archive>`: CLI extract
