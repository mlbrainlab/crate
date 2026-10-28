# Changelog

Every finished task bumps the version (see "Versions" in `docs/ROADMAP.md`): the patch number per task, 0.1.0 for the first Linux release (M6), 0.2.0 Windows (M7), 0.3.0 macOS (M8), and 1.0.0 with the final milestone (M9).

## 0.1.5 — 2026-10-28
- Windows: extraction, tests and edits show their progress on the taskbar button.
- Windows: First run and Preferences → "Open these with Crate" open Settings → Default apps on Crate's page (Windows lets only you choose default apps), and Preferences shows which types already open with Crate.
- Windows: `crate-app.exe` carries Crate's icon.

## 0.1.4 — 2026-10-28
- Windows: files extracted from a downloaded archive keep the "downloaded from the internet" mark, as with Explorer's own Extract, so SmartScreen and Office's Protected View still check them.

## 0.1.3 — 2026-10-28
- Names a Windows disk won't take (`CON.txt`, `report: v2?.pdf`, `notes.`) are found before extracting: Crate shows the new names and lets you rename, edit the names, or skip those files. Very long paths extract in long-path mode.
- Two files whose names differ only by case (`README.md`, `readme.md`) no longer overwrite each other on Windows and macOS: keep both, keep the newer one, or skip.
- The `crate` tool says which names it changed.

## 0.1.2 — 2026-10-28
- Windows: Crate builds and its whole test suite runs on Windows in CI. RAR archives open by their full Unicode path there.

## 0.1.1 — 2026-10-28
- The busy cursor no longer lingers after Crate's window opens: the window now tells the desktop the launch is over (startup notification on Wayland and X11).

## 0.1.0 — 2026-10-28
The first release, for Linux.

- **Open anything common:** ZIP, 7z, RAR and tar (gz, xz, zst, bz2), including split sets (`.7z.001`, `.part1.rar`) and archives inside archives. Make ZIP, 7z and tar archives, with passwords (AES-256) and split volumes.
- **One click to extract:** double-click an archive for the mini view, or browse, preview and edit it in the full view. Batch extract several at once; long jobs show in the Operations panel and notify you when they finish in the background.
- **Safe by default:** files that try to leave the folder, links pointing outside it and decompression bombs are stopped and reported; archives are never rewritten in place.
- **Readable everywhere:** garbled names in old ZIPs are fixed (and remembered), the interface is in English and Arabic (right to left), light and dark, usable with the keyboard and a screen reader.
- **Install:** `.deb`, `.rpm` or AppImage below; the `crate` command-line tool comes with them.

## 0.0.9 — 2026-10-28
M6: fuzzing.
- The ZIP, tar, tar.gz and 7z readers are fuzzed (cargo-fuzz): every input is opened, listed, tested and extracted, and nothing may land outside the destination. 1.8 million inputs so far, no problems; CI fuzzes on every push.

## 0.0.8 — 2026-10-28
M6: for contributors.
- Issue templates for bugs and ideas; security problems are reported privately.
- CONTRIBUTING.md: how the repository is organised, the checks, the rules for pull requests, adding a translation.
- `crate-app --version`.

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
