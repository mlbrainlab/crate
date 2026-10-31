# Changelog

Every finished task bumps the version (see "Versions" in `docs/ROADMAP.md`): the patch number per task, 0.1.0 for the first Linux release (M6), 0.2.0 Windows (M7), 0.3.0 macOS (M8), and 1.0.0 with the final milestone (M9).

## 0.2.19 — 2026-10-31
- Pro (M9): Preferences has a Crate Pro group. Upgrade… opens a sheet that says what Pro adds ($9 once), with Buy (once the store exists) and "I have a license key". Entering a key activates Pro, and Remove license takes it off. Nothing in Crate is locked yet: the Glass theme and the extra icons, the only Pro features, are still to come.

## 0.2.18 — 2026-10-31
- Pro (M9), groundwork: the license check. A Crate Pro key is signed with Crate's private key and checked offline with the public key in the app. A Lemon Squeezy key is exchanged once for a signed key through a small signer (`signer/`); nothing is sent after that. Nothing in the app uses it yet.

## 0.2.17 — 2026-10-31
- macOS: making Crate the default app waits for macOS's answer, so Preferences shows the new state right away and a failure shows as an error instead of nothing happening. The first-run screen and Preferences no longer say macOS asks you to confirm: it doesn't, for file types.

## 0.2.16 — 2026-10-31
- macOS: a Window menu in the menu bar, with Minimize (⌘M) and Zoom, in the home and archive windows.

## 0.2.15 — 2026-10-31
- Full view: long folder and archive names in the breadcrumbs shorten with "…" instead of pushing the file list past the window's right edge (the Packed, Ratio and Modified columns were cut off unless the window was maximized).
- Full view: in a narrow window the toolbar shows its buttons' icons only, so it fits down to the window's minimum width.

## 0.2.14 — 2026-10-31
- Windows: Crate starts on a fresh Windows. The Visual C++ runtime is now built into the binaries instead of needing VCRUNTIME140.dll and MSVCP140.dll, and packaging fails if a binary needs them again.

## 0.2.13 — 2026-10-31
- Tests: the CLI's Ctrl+C test no longer loses the race against a fast disk (macOS runners finished the extract before the signal arrived). With `CRATE_TEST_SLOW` set, the CLI pauses at every progress step, and the test signals the process directly.
- Tests: the UI tests wait until an archive has been read completely, not just until its first entries show. Extract does nothing before then, which made `names_flow` time out about one run in five.

## 0.2.12 — 2026-10-31
- macOS: Finder's keys in an archive: ⌘↓ opens the item under the cursor, ⌘↑ goes up a folder, ⌘⌫ deletes the selection from the archive, and Return renames (it still opens in archives Crate can't edit). The shortcuts sheet already listed them.

## 0.2.11 — 2026-10-31
- macOS: the traffic lights share the header buttons' centre line (the header's height now comes from where the lights actually are).
- macOS: Preferences puts the traffic lights in its header too, so its title shows once.
- macOS: the Operations page's Back button moves clear of the traffic lights.
- The header title is centred again, as in the mockups (0.2.9 had moved it to the left edge).

## 0.2.10 — 2026-10-31
- macOS: the traffic lights line up with the header's buttons (the title bar is now a unified toolbar's height, and the header takes on that height), and stay in place through resizing and full screen.
- macOS: Home's cards show ⌘N and ⌘O instead of Ctrl N and Ctrl O.

## 0.2.9 — 2026-10-30
- macOS: Home and the full view put the traffic lights in Crate's own header, next to its buttons, as current Mac apps do; the mini view keeps the plain title bar.
- macOS: the UI uses the system's fonts (San Francisco, and SF Arabic for the Arabic UI) instead of the bundled Inter and IBM Plex Sans Arabic.

## 0.2.8 — 2026-10-30
- macOS: the app and the `crate` CLI no longer fail to start on Macs without Homebrew's xz: xz is built into them, and packaging now fails if a binary needs any library macOS doesn't ship.

## 0.2.7 — 2026-10-30
- macOS: releases sign the app and the `.dmg` with Crate's Developer ID certificate, notarize the `.dmg` with Apple and staple the ticket, so it opens without the "unidentified developer" warning; the `.dmg` is now part of each GitHub Release (`docs/macos-signing.md`).

## 0.2.6 — 2026-10-29
- Homebrew: `scripts/homebrew-cask.py` writes the cask (`crate-archiver`) for a published version, and `docs/homebrew.md` has the ways to publish it.

## 0.2.5 — 2026-10-29
- macOS: when macOS hasn't let Crate into a folder, the "can't write" card says where to allow it and opens Privacy & Security → Files and Folders; macOS's own prompts explain why Crate asks, in English and Arabic.

## 0.2.4 — 2026-10-29
- macOS: Crate's menus are in the menu bar: Crate (About, Preferences, Quit), File, Edit, Archive, View and Help, with their ⌘ shortcuts.

## 0.2.3 — 2026-10-29
- macOS: first run and Preferences make Crate the default app for the archive types you pick; macOS asks you to confirm each one.

## 0.2.2 — 2026-10-29
- macOS: Crate opens the archives Finder sends it (double-click, Open With), and extraction progress shows on the Dock icon.

## 0.2.1 — 2026-10-29
- macOS builds: every change is now built and tested on macOS in CI, and `scripts/package-macos.sh` makes a universal `Crate.app` (Apple Silicon and Intel, macOS 12 and later) in a `.dmg`, with Crate listed in Finder's "Open With" for the same archive types as on Windows. Not in the release yet: it comes once it's signed and notarized.

## 0.2.0 — 2026-10-29
Crate for Windows.

- **Install:** `Crate-0.2.0-x64-setup.exe` below, for Windows 10 and 11. It needs no administrator rights (or installs for everyone if you choose), and adds Crate to "Open with", Settings → Default apps and Explorer's right-click menu (Open with Crate, Extract with Crate, Compress with Crate…). It isn't code-signed yet: SmartScreen asks once, "More info → Run anyway"; `SHA256SUMS` has its checksum.
- **At home on Windows:** the native title bar with Snap Layouts, progress on the taskbar button, Downloads and Desktop wherever Windows keeps them (OneDrive too), and paths shown as Explorer shows them.
- **Safe on Windows:** names Windows won't accept (`CON.txt`, `report: v2?.pdf`, `notes.`) are found before extracting, to rename, edit or skip; files that differ only by case don't overwrite each other; very long paths work; extracted files keep the "downloaded from the internet" mark, so SmartScreen and Office still check them.
- **Everywhere:** the `crate` tool says which names it changed, and the same checks run on Windows in CI for every change.

## 0.1.7 — 2026-10-28
- winget: `scripts/winget-manifest.py` writes the manifests for a published version, and `docs/winget.md` has the steps to submit them.

## 0.1.6 — 2026-10-28
- Windows installer (`Crate-…-x64-setup.exe`): installs for you alone without administrator rights, or for everyone. It adds Crate to "Open with" and to Settings → Default apps for zip, 7z, rar and tar files, and puts "Open with Crate", "Extract with Crate" and "Compress with Crate…" in Explorer's right-click menu.
- Releases build the Windows installer next to the Linux packages, after installing and uninstalling it once on a clean runner.

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
- Windows: settings live in AppData, Downloads and Desktop follow the system (OneDrive too), paths show as Explorer shows them, archives can be edited while their window is open, and links to files extract as copies.

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
