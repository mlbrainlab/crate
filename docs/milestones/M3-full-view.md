# M3: Full view: browse

**Goal:** the full archive browser for exploring and partially extracting archives.
**Done when:** you can browse any test archive, select files, preview them and extract a selection on Ubuntu.

## Tasks
- [x] Switch between mini and full view (Ctrl+Enter, expand button); remember the full view's window size
- [x] Sidebar folder tree, breadcrumbs, file list with Size / Packed / Ratio / Modified
- [x] Multi-select (click, Shift, Ctrl), keyboard navigation, context menu (`ContextMenu.dc.html`, in-app part only)
- [x] Details pane: CRC, method, sizes; Copy path
- [x] Filter / search in the current archive
- [x] Only draw visible rows (Slint `ListView` / a `VecModel` that grows); listing streams in for tar archives (`HugeArchive.dc.html`)
- [x] Quick preview with Space: text/code, images, hex for binaries (`Preview.dc.html`)
- [x] Grid view with thumbnails + auto-switch for media folders (`Grid.dc.html`)
- [x] Extract dialog (`Extract.dc.html`): what to extract, destination, if-file-exists rule, open when done
- [x] File already exists prompt (`Dialogs.dc.html`), disk-full and permission errors (`FileIssues.dc.html`)
- [x] Drag files out of the archive to Files / desktop (`DragOut.dc.html`). Check first whether Slint/winit can start a drag to other apps (BACKLOG 2.1); if it needs platform code that is too large for M3, log it in the backlog and move on

## Notes
- **One window, two densities:** `ArchiveWindow` shows the mini view or the full view (Ctrl+Enter, the expand / collapse buttons). The full view’s size is kept in `~/.config/crate/state.ini`. State lives in the `ArchiveUi` global (`app/ui/state.slint`); behavior in `app/src/view/`.
- **Huge archives:** tar listings stream in batches (`Archive::open_streaming`); the list, tree and search work on what has arrived, and **Stop reading** keeps the first part (still extractable). ZIP and 7z have an index and list at once.
- **Reading while browsing:** `Archive::try_clone` gives preview, thumbnails and Test their own handle, so the window stays usable.
- **Extract dialog** maps to new engine options: Keep both / Keep newer / Ask (`IfExists`, with a resolver the worker calls; the UI answers “File already exists”), and extracting without folders (`keep_structure`). Browse uses the XDG portal (`rfd`), Move to Trash the freedesktop trash (`trash`).
- **File issues:** disk full and permission denied have their own dialogs. Windows reserved names and case clashes (`FileIssues.dc.html`) wait for M7/M8.
- **Drag out is blocked** by the toolkit: see BACKLOG 2.1. Nothing in M3 depends on it.
- **Left for M4** (editing): Add files, New folder, Rename, Delete, Move to folder in the toolbar and context menu. “Open with” another app and Properties (archive) come with M5.
- **Tests:** `app/tests/full_flow.rs` drives the full view (folders, sorting, details, search, clipboard, Test, Extract dialog with conflicts and a read-only folder, preview, grid, a 120k-entry tar streaming in) and saves screenshots in `target/ui-snapshots/`.

## Design references
- `Browse.dc.html`, `Extract.dc.html`, `Preview.dc.html`, `Grid.dc.html`, `HugeArchive.dc.html`, `ContextMenu.dc.html`, `FileIssues.dc.html`

## Acceptance (on Ubuntu)
- [x] A 100k-entry tar.zst opens, scrolls smoothly, and search works while it loads
- [x] Extracting 3 selected files puts only those 3 in the destination
- [x] Space previews a .png, a .rs file and a binary
- [x] Extracting into a read-only folder shows the permission dialog, not a crash

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M3-full-view.md. Reuse the M2 component in "full" density. Build in this order: list → tree → selection → extract dialog → preview → grid → drag-out.
