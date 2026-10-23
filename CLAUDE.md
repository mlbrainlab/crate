# Crate: notes for Claude Code

Crate is a cross-platform archiver (Tauri 2 + Rust). This is an open-source showcase project with a small optional Pro tier.

## Where things are
- Machines, builds and parallel sessions: `docs/WORKFLOW.md`.
- Build plan: `docs/ROADMAP.md`. Work on **one milestone at a time**: `docs/milestones/M*.md`.
- App icon: `assets/icons/` (concept #1 "Open Lid"; see its README for Tauri usage).
- Design: `design/DESIGN.md` (colors, rules, per-platform notes), `design/BACKLOG.md` (status of every item), mockups in `design/canvas/*.dc.html`. The mockups are plain HTML; open them in a browser or read them for exact spacing and copy.

## Working rules
- Linux (Ubuntu) first. Don't add Windows/macOS-specific code before M7/M8, except `cfg` stubs.
- Keep `crate-core` free of UI and Tauri dependencies. The app and the CLI both call it.
- Every file write during extraction goes through `crate_core::safety`: no path escapes, no symlinks pointing outside, decompression-bomb limits. Never bypass it.
- Never modify an original archive in place: write to a temp file in the same folder, then rename it over the original.
- Long operations run off the UI thread, report progress events, and can be cancelled.
- Use the color variables from `design/DESIGN.md`, not new hex values.
- Commit after each working slice. Tick the checkboxes in the milestone file as you go.
- If a task needs a decision listed under "Open decisions" in the ROADMAP, stop and ask.

## Commands
- `pnpm tauri dev`: run the app
- `cargo test --workspace`: all Rust tests
- `cargo run -p crate-cli -- x <archive>`: CLI extract
