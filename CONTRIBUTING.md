# Contributing to Crate

Thanks for your interest! The project is still in early development, so expect things to move.

## Before you start
- Read [docs/ROADMAP.md](docs/ROADMAP.md). Work happens one milestone at a time (`docs/milestones/M*.md`); features from later milestones go into `design/BACKLOG.md`, not into the code.
- The design lives in `design/` (`DESIGN.md` for colors and rules, mockups in `design/canvas/`).

## Rules
- `crate-core` has no UI (Slint) dependencies; the app and the CLI both call it.
- Every file written during extraction goes through `crate_core::safety`. Never bypass it.
- Never modify an original archive in place: write a temp file next to it, then rename it over the original.
- Long operations run off the UI thread, report progress and can be cancelled.
- Use the color tokens from `design/DESIGN.md` (`Tokens` in `app/ui/tokens.slint`); don't add new hex values.
- Linux first: no Windows/macOS-specific code before M7/M8 except `cfg` stubs.

## Checks
Before opening a pull request:
```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
