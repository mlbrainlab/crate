# M3: Full view: browse

**Goal:** the full archive browser for exploring and partially extracting archives.
**Done when:** you can browse any test archive, select files, preview them and extract a selection on Ubuntu.

## Tasks
- [ ] Switch between mini and full view (Ctrl+Enter, expand button); remember the full view's window size
- [ ] Sidebar folder tree, breadcrumbs, file list with Size / Packed / Ratio / Modified
- [ ] Multi-select (click, Shift, Ctrl), keyboard navigation, context menu (`ContextMenu.dc.html`, in-app part only)
- [ ] Details pane: CRC, method, sizes; Copy path
- [ ] Filter / search in the current archive
- [ ] Only draw visible rows; listing streams in for tar archives (`HugeArchive.dc.html`)
- [ ] Quick preview with Space: text/code, images, hex for binaries (`Preview.dc.html`)
- [ ] Grid view with thumbnails + auto-switch for media folders (`Grid.dc.html`)
- [ ] Extract dialog (`Extract.dc.html`): what to extract, destination, if-file-exists rule, open when done
- [ ] File already exists prompt (`Dialogs.dc.html`), disk-full and permission errors (`FileIssues.dc.html`)
- [ ] Drag files out of the archive to Files / desktop (`DragOut.dc.html`). Check Tauri drag-out support first; if it's missing, log it in the backlog and move on

## Design references
- `Browse.dc.html`, `Extract.dc.html`, `Preview.dc.html`, `Grid.dc.html`, `HugeArchive.dc.html`, `ContextMenu.dc.html`, `FileIssues.dc.html`

## Acceptance (on Ubuntu)
- [ ] A 100k-entry tar.zst opens, scrolls smoothly, and search works while it loads
- [ ] Extracting 3 selected files puts only those 3 in the destination
- [ ] Space previews a .png, a .rs file and a binary
- [ ] Extracting into a read-only folder shows the permission dialog, not a crash

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M3-full-view.md. Reuse the M2 component in "full" density. Build in this order: list → tree → selection → extract dialog → preview → grid → drag-out.
