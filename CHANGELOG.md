# Changelog

Every finished task bumps the version (see "Versions" in `docs/ROADMAP.md`): the patch number per task, 0.1.0 for the first Linux release (M6), 0.2.0 Windows (M7), 0.3.0 macOS (M8), and 1.0.0 with the final milestone (M9).

## 0.0.7 — 2026-10-28
M6: README.
- A new README: screenshots, what Crate does, install table, how it's built, real benchmarks (`scripts/bench-cli.sh`).
- Creating a tar.zst uses every core (about twice as fast on 1 GB).
- New archive shows the output path as `~/…`.

## 0.0.6 — 2026-10-28
M6: Linux packages.
- .deb, .rpm and AppImage builds (`scripts/package-linux.sh`), named `crate-archiver`, with the `crate` command-line tool, icons, the desktop entry and AppStream metadata.
- Built on Ubuntu 22.04 so they run on most current distributions; checked on Ubuntu 24.04 and Fedora 44.
- A release workflow publishes them from a version tag, with checksums.

## 0.0.5 — 2026-10-28
M5: operations panel. Every M5 task is done.
- Home has an Operations page listing every job of every Crate window: progress, time left, current file, speed, with Pause / Resume and Cancel; queued archives of a batch can be removed.
- Finished jobs are kept in a history with Open / Show in folder; Clear finished empties it.
- A job that ends while its window is in the background sends a desktop notification.

## 0.0.4 — 2026-10-27
M5: nested archives and missing volumes.
- Split archives (`name.7z.001`, `.002`…, the kind Crate's own "Split into volumes" makes) open from any part; RAR sets (`.part1.rar`, `.r00`) are checked for gaps.
- A missing part says which one, lists the parts found, lets you locate it in another folder, and the window opens the archive by itself once the part appears (a download finishing). Batch extract can locate it too.
- An archive inside an archive opens in Crate; edits come back through "Update archive".
- New in the Extract dialog: "Also unpack archives found inside" (up to 3 levels, every level with the usual safety checks).

## 0.0.3 — 2026-10-27
M5: batch extract.
- Pick or drop several archives on Home, or run `crate-app --extract-each <archives…>`: Crate extracts them one after another, each into its own folder (or all into one), with overall progress and time left.
- Archives that need a password wait for it without holding up the rest; damaged ones show the error with Retry; any row can be paused, opened when done, or skipped.
- Optionally moves each archive to the Trash once it's extracted.

## 0.0.2 — 2026-10-27
M5: accessibility pass.
- Every control can be reached with Tab and pressed with Space or Enter, with a focus ring that shows only for keyboard users; Up / Down change drop-downs and segmented controls.
- Screen readers get a name and role for every control, including icon buttons, checkboxes, switches, tabs and list rows (with size and selected state); toasts are announced.
- A test fails if any control on the main screens has no name.

## 0.0.1 — up to 2026-10-27
Everything before per-task versions:
- M0–M4: the Slint app, safe listing and extraction (`crate-core`), the CLI, the mini view, the full view with preview and grid, creating archives, editing inside archives, Properties, testing and converting.
- M5 so far: mini view folder navigation, Preferences, First run with "Make Crate the default", the Arabic UI with a mirrored layout, the keyboard shortcuts sheet, dark mode on every screen, the filename encoding picker with "Remember for this archive", and the About & support screen.
