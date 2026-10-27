# Contributing to Crate

Thanks for helping. Bug reports, translations and pull requests are all welcome.

## Reporting a bug

Open an issue with the **Bug report** template. The most useful things to include:

- Crate's version (About, or `crate-app --version`), your distribution and desktop (GNOME, KDE…), X11 or Wayland.
- What you did, what you expected, what happened.
- If an archive misbehaves and you can share it, attach it (or a small one that shows the same problem). If you can't share it, `crate l <archive>` and `crate t <archive>` output often helps.

**Security problems** (an archive that writes outside its folder, follows a link out, or crashes Crate in a way that looks exploitable): please don't open a public issue. Use GitHub's private vulnerability reporting (Security → Report a vulnerability).

## How the project is organised

| Where | What |
|---|---|
| `crates/crate-core` | The archive engine: no UI code. Every file written during extraction goes through `crate_core::safety` |
| `crates/crate-cli` | The `crate` command-line tool |
| `app/` | The desktop app: `app/ui/*.slint` (UI), `app/src/` (controllers), `app/tests/` (headless UI flows) |
| `docs/` | `ROADMAP.md`, one file per milestone in `docs/milestones/`, `perf.md` |
| `design/` | `DESIGN.md` (colors, rules), `BACKLOG.md`, the mockups in `design/canvas/*.dc.html` |
| `packaging/`, `scripts/` | Linux packaging, the desktop entry, benchmarks |

Work follows the milestones in `docs/ROADMAP.md`, one at a time. Ideas for later milestones go to `design/BACKLOG.md`.

## Building and testing

```bash
sudo apt install build-essential libfontconfig1-dev libxkbcommon-dev libwayland-dev
cargo test --workspace
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
```

CI runs the same three. UI flows (`cargo test -p crate-app --test mini_flow`, `full_flow`, …) render with Slint's software renderer and save screenshots to `target/ui-snapshots/`; `CRATE_UI_DARK=1` runs them in dark mode.

## Pull requests

- Keep each change small and about one thing; describe what changed and how you checked it.
- Add or update a test with every fix and feature.
- Never weaken the safety rules: no path escapes, no links pointing outside, decompression-bomb limits, and never modify an original archive in place (write next to it, then rename).
- `crate-core` has no UI (Slint) dependencies; the app and the CLI both call it.
- Long operations run off the UI thread, report progress and can be cancelled.
- Linux first: no Windows- or macOS-specific code before M7 / M8, except `cfg` stubs.
- Colors come from the `Tokens` global (`app/ui/tokens.slint`), not new hex values.
- Every string people see is translatable: `@tr()` in `.slint`, `i18n::tr` / `trf` / `trn` in Rust, no sentences glued together. Run `scripts/update-translations.sh` afterwards.
- Layouts must also work right to left (Arabic): rows with two or more children are `GridLayout`s with `col: Locale.rtl ? … : …`.

## Translations

Translations live in `app/translations/<language>/LC_MESSAGES/crate-app.po` (gettext). To add a language, copy `app/translations/crate-app.pot` there, translate, and open a pull request; for a right-to-left language, say so, since the layout direction comes from the catalog.

## License

By contributing you agree that your work is released under the [MIT license](LICENSE).
