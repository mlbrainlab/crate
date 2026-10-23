# M7: Windows port

**Goal:** Crate on Windows 10/11 with the same features. Shipped unsigned.
**Done when:** the .msi/.exe from GitHub Releases installs and works on Windows 11, and the winget manifest is submitted.

## Tasks
- [ ] Build and CI on `windows-latest`
- [ ] Window frame: Crate's own header + caption buttons on the right; check whether Snap Layouts can be kept (e.g. `tauri-plugin-decorum`)
- [ ] File names: reserved names, illegal characters, long paths, names that differ only by case (`FileIssues.dc.html`)
- [ ] Copy the "downloaded from the internet" flag (Mark-of-the-Web, `Zone.Identifier`) onto extracted files
- [ ] File associations in the installer (ProgIDs → Crate appears in "Open with")
- [ ] Default app: Windows doesn't let apps set themselves as default. First run opens `ms-settings:defaultapps?registeredAppUser=Crate` (verify the exact registered name)
- [ ] Explorer right-click: classic registry verbs (they appear under "Show more options"); the modern Windows 11 menu is post-v1
- [ ] Taskbar progress
- [ ] Unsigned-build README section ("More info → Run anyway") + checksums
- [ ] winget manifest

## Acceptance (Windows 11 VM)
- [ ] Install → first run → Default apps page opens with Crate listed
- [ ] Extracting an archive containing `CON.txt` and a 300-character path shows the rename prompt and succeeds
- [ ] Files extracted from a downloaded zip keep the "downloaded from the internet" flag

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M7-windows.md. Keep Windows-only code behind `#[cfg(windows)]` in a `platform` module.
