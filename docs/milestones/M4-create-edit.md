# M4: Create & edit

**Goal:** create archives and change existing ones safely.
**Done when:** you can compress a folder, edit a zip's contents, and verify it, all on Ubuntu.

## Decision to make first
- [ ] Passwords: ZIP (AES-256) + 7z only (default), or also tar.* with age. First verify the chosen crates can **write** encrypted archives.

## Tasks
### Create (`Create.dc.html`)
- [ ] Formats: ZIP, tar.zst, tar.gz, tar.xz, 7z (if writing is supported)
- [ ] Level slider with an estimated size, exclude patterns, keep permissions/symlinks, split into volumes (ZIP/7z)
- [ ] Password + strength meter (formats from the decision above)
- [ ] Presets (save / load)
- [ ] Drop files/folders on Home to start a new archive (`Main.dc.html`, `AddFiles.dc.html`)

### Edit an existing archive (always rewrite to a temp file, then rename over the original)
- [ ] Add files (drag in, Add button), new folder, rename, delete (+ optional .bak backup)
- [ ] Open → edit → update: watch the temp copy and show the "Update the archive?" prompt (`EditUpdate.dc.html`)
- [ ] Read-only formats (RAR, ISO): offer "Save copy elsewhere…" instead

### Inspect
- [ ] Test integrity (CRC); Properties with stats and type breakdown (`Properties.dc.html`)
- [ ] Archive checksums SHA-256 / BLAKE3 / MD5 + paste a checksum to verify
- [ ] Convert format (`Dialogs.dc.html`)

## Acceptance (on Ubuntu)
- [ ] A folder compresses to tar.zst and extracts back byte-for-byte identical (test compares hashes)
- [ ] Deleting a file inside a zip, then killing the app mid-rewrite, leaves the original zip intact
- [ ] Editing a .txt inside a zip in Text Editor, saving, and accepting "Update" puts the change in the zip

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M4-create-edit.md. Implement every write operation as "build a new archive in a temp file next to the original → fsync → rename", with a crash-safety test for each.
