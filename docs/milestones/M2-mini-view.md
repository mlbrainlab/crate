# M2: Mini view

**Goal:** double-clicking an archive in Ubuntu Files opens the mini view, and one click extracts everything.
**Done when:** the full double-click → extract → "Open folder & close" flow works on Ubuntu.

## Tasks
- [ ] Open a file passed on the command line or through a file association (`crate path/to/file.zip`); allow only one window per archive
- [ ] Linux `.desktop` file with `MimeType=` for zip, 7z, rar, tar and its variants + `scripts/dev-install-desktop.sh`, which installs it into `~/.local/share/applications` for testing
- [ ] Mini view UI per `MiniView.dc.html` (Linux frame): summary, chips, search, top-level list, click to select
- [ ] Primary button: "Extract all to “name/”" or "Extract N selected"; fills with live progress; Cancel
- [ ] Smart folder: no extra wrapper folder when the archive has a single top-level folder
- [ ] Done popup: **Open folder & close** · Open folder · Back to archive · Close window
- [ ] Open the folder with the system file manager (`xdg-open` / Tauri opener plugin)
- [ ] Password prompt for encrypted entries (`Dialogs.dc.html`)
- [ ] Garbled-name banner + "Fix names" when the encoding guess is not UTF-8 (`MiniViewRTL.dc.html`, LTR for now)

## Design references
- `MiniView.dc.html`, `Dialogs.dc.html` (password), `MiniViewRTL.dc.html` (encoding banner)

## Acceptance (on Ubuntu)
- [ ] After `scripts/dev-install-desktop.sh`, right-click a .zip → Open With → Crate opens the mini view
- [ ] Extracting a 1 GB+ archive keeps the UI responsive and the progress smooth
- [ ] "Open folder & close" opens Files at the right folder and closes Crate
- [ ] An encrypted zip asks for the password; a wrong password shows the error state

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M2-mini-view.md, then open design/canvas/MiniView.dc.html for exact layout and copy. Build the mini view as a component that can later run in "compact" or "full" density. Use Tauri events for progress.
