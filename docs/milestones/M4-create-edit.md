# M4: Create & edit

**Goal:** create archives and change existing ones safely.
**Done when:** you can compress a folder, edit a zip's contents, and verify it, all on Ubuntu.

## Decision to make first
- [x] Passwords: ZIP (AES-256) + 7z only (default), or also tar.* with age. First verify the chosen crates can **write** encrypted archives. **Decided 2026-10-25: also tar.* with age**; writing verified for all three (BACKLOG 1.10, 1.11).

## Tasks
### Create (`Create.dc.html`)
- [x] Formats: ZIP, tar.zst, tar.gz, tar.xz, 7z (if writing is supported)
- [x] Level slider with an estimated size, exclude patterns, keep permissions/symlinks, split into volumes (ZIP/7z)
- [x] Password + strength meter (formats from the decision above)
- [x] Presets (save / load)
- [x] Drop files/folders on Home to start a new archive (`Main.dc.html`, `AddFiles.dc.html`)

### Edit an existing archive (always rewrite to a temp file, then rename over the original)
- [x] Add files (drag in, Add button), new folder, rename, delete (+ optional .bak backup)
- [x] Open → edit → update: watch the temp copy and show the "Update the archive?" prompt (`EditUpdate.dc.html`)
- [x] Read-only formats (RAR, ISO): offer "Save copy elsewhere…" instead

### Inspect
- [x] Test integrity (CRC); Properties with stats and type breakdown (`Properties.dc.html`)
- [x] Archive checksums SHA-256 / BLAKE3 / MD5 + paste a checksum to verify
- [x] Convert format (`Dialogs.dc.html`)

## Notes
- **Engine:** `crate_core::create` (ZIP Deflate + AES-256; 7z LZMA2 in solid blocks + AES-256 with hidden names; tar / gz / xz / zst / bz2; volumes as `.001`, `.002`…), `crate_core::edit` (add, new folder, rename, delete, replace; ZIP entries copied raw, unchanged 7z blocks copied raw, tar and age rewritten), `crate_core::convert`, `crate_core::checksum`. Every write goes to `.crate-new-…` next to the target, is fsynced and renamed over it; `name.bak` optional.
- **Passwords for tar** use age (decided 2026-10-25): `name.tar.zst.age`, passphrase via scrypt. Opening one asks for the passphrase before listing.
- **CLI:** `crate c / a / d / rn / sum / convert` (see `crate --help`).
- **Drag and drop is blocked by the toolkit** in both directions (BACKLOG 2.1): Slint ignores drops from other apps, and winit has none on Wayland. Home and the full view use the desktop's file chooser (XDG portal) for Compress files, Add files and Add folder instead. `crate-app --compress <files>` opens New archive directly (for a future Files "Compress…" entry).
- **Volumes** are plain splits (`.7z.001`…), the convention 7-Zip reads. Crate itself reads split sets in M5.
- **A hard kill mid-write** leaves the original intact (tested with SIGKILL) but also a stray `.crate-new-…` file next to it; it isn't removed automatically, since another Crate could be writing it.
- **Left out:** the Properties "Comment" tab (comments aren't read yet; only ZIP has them); ISO (not a supported format yet).
- **Tests:** `crates/crate-core/tests/{create,edit,convert}.rs`, CLI acceptance tests in `crates/crate-cli/tests/cli.rs`, and `app/tests/{home_flow,full_flow,mini_flow}.rs` with screenshots.

## Acceptance (on Ubuntu)
- [x] A folder compresses to tar.zst and extracts back byte-for-byte identical (test compares hashes)
- [x] Deleting a file inside a zip, then killing the app mid-rewrite, leaves the original zip intact
- [x] Editing a .txt inside a zip in Text Editor, saving, and accepting "Update" puts the change in the zip

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M4-create-edit.md. Implement every write operation as "build a new archive in a temp file next to the original → fsync → rename", with a crash-safety test for each.
