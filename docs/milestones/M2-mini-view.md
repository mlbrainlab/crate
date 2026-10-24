# M2: Mini view

**Goal:** double-clicking an archive in Ubuntu Files opens the mini view, and one click extracts everything.
**Done when:** the full double-click → extract → "Open folder & close" flow works on Ubuntu.

## Tasks
- [x] Open a file passed on the command line or through a file association (`crate path/to/file.zip`); allow only one window per archive
- [x] Linux `.desktop` file with `MimeType=` for zip, 7z, rar, tar and its variants + `scripts/dev-install-desktop.sh`, which installs it into `~/.local/share/applications` for testing
- [x] Mini view UI per `MiniView.dc.html` (Linux frame): summary, chips, search, top-level list, click to select. Start from `app/ui/mini-view.slint` (M0's static spike)
- [x] Primary button: "Extract all to “name/”" or "Extract N selected"; fills with live progress; Cancel
- [x] Smart folder: no extra wrapper folder when the archive has a single top-level folder
- [x] Done popup: **Open folder & close** · Open folder · Back to archive · Close window
- [x] Open the folder with the system file manager (`xdg-open`, e.g. through the `open` crate)
- [x] Password prompt for encrypted entries (`Dialogs.dc.html`)
- [x] Extend `scripts/startup-bench.sh` to open a real archive in the mini view (listing included) and re-measure; it must stay within the budget in ROADMAP
- [x] Garbled-name banner + "Fix names" when the encoding guess is not UTF-8 (`MiniViewRTL.dc.html`, LTR for now)

## Notes
- **Run:** `crate-app <archive>` opens the mini view; without an argument the (empty) full window. The archive is read on a worker thread, so the window never waits for a slow listing (a big tar.gz has to be decompressed to be listed).
- **One window per archive:** the first process listens on a socket in `$XDG_RUNTIME_DIR/crate/`, named after the archive's resolved path; a second open asks it to raise its window and exits. GNOME may show "Crate is ready" instead of raising, since a new process can't hand over its activation token through winit.
- **Smart folder** lives in `crate_core::destination`: one top-level folder is extracted as is next to the archive; anything else goes into a folder named after the archive. Existing folders are never merged into: `name (2)` follows `name`.
- **Passwords** (`crate-core`): ZIP (AES and ZipCrypto), 7z (including encrypted file lists) and RAR. The prompt checks the password on the smallest encrypted file before extracting, so a wrong one is reported at once, and a wrong one never leaves files behind.
- **Encoding banner:** the guess is already applied, so the banner says which encoding was guessed and "Fix names" opens the picker (`MiniViewRTL.dc.html`) with a live preview.
- **Left out on purpose:** the split button's chevron and "Extract to…" (the Extract dialog is M3); the hamburger menu and the expand button (M3); "Remember in system keyring" (not planned in any milestone yet); the "Tested OK" chip (testing on open would cost startup time; it can come with the full view's Test action).
- **Selection** follows desktop rules rather than the mockup's click-to-toggle: click selects one row, Ctrl+click toggles, Shift+click selects a range, Ctrl+A all, arrows / Page Up / Page Down / Home / End move (Shift extends), Space toggles, Escape clears. Enter extracts.
- **Double-click opens an item:** it is extracted (through the same safety checks and password prompt) into a private folder under `~/.cache/crate/` and opened with the default app; a folder opens in Files. Those copies are removed when Crate closes, like File Roller does.
- **Big ZIPs list instantly:** the `zip` crate's default scans the whole file for signatures (1 s per GB); Crate reads the archive at offset 0 first and only falls back to the scan for self-extracting style files. Compressed tarballs still have to be decompressed to be listed (as with File Roller); the window shows "Reading archive…" meanwhile.
- **Tests:** `app/tests/mini_flow.rs` drives the real controller on Slint's software renderer (open, search, select, extract, wrong and right passwords, encrypted file list, Fix names, damaged archive, bomb, cancel) and writes screenshots to `target/ui-snapshots/`.

## Design references
- `MiniView.dc.html`, `Dialogs.dc.html` (password), `MiniViewRTL.dc.html` (encoding banner)

## Acceptance (on Ubuntu)
- [ ] After `scripts/dev-install-desktop.sh`, right-click a .zip → Open With → Crate opens the mini view
- [ ] Extracting a 1 GB+ archive keeps the UI responsive and the progress smooth
- [ ] "Open folder & close" opens Files at the right folder and closes Crate
- [ ] An encrypted zip asks for the password; a wrong password shows the error state

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M2-mini-view.md, then open design/canvas/MiniView.dc.html for exact layout and copy. Build the mini view as a Slint component that can later run in "compact" or "full" density. Run extraction on a worker thread and push progress to the UI with `slint::Weak::upgrade_in_event_loop`.
