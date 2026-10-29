# M7: Windows port

**Goal:** Crate on Windows 10/11 with the same features. Shipped unsigned.
**Done when:** the .msi/.exe from GitHub Releases installs and works on Windows 11, and the winget manifest is submitted.

## Tasks
- [x] Build and CI on `windows-latest`
- [x] Window frame: Crate's own header + caption buttons on the right; check whether Snap Layouts can be kept with a frameless winit window (hit-testing the maximize button). **Decided 2026-10-29: the native title bar** (DESIGN.md, MiniView/PlatformFull mockups); Crate's own frame comes with the Glass theme in M9
- [x] File names: reserved names, illegal characters, long paths, names that differ only by case (`FileIssues.dc.html`)
- [x] Copy the "downloaded from the internet" flag (Mark-of-the-Web, `Zone.Identifier`) onto extracted files
- [x] File associations in the installer (ProgIDs → Crate appears in "Open with")
- [x] Default app: Windows doesn't let apps set themselves as default. First run opens `ms-settings:defaultapps?registeredAppUser=Crate` (verify the exact registered name)
- [x] Explorer right-click: classic registry verbs (they appear under "Show more options"); the modern Windows 11 menu is post-v1
- [x] Taskbar progress
- [x] Unsigned-build README section ("More info → Run anyway") + checksums
- [x] winget manifest

## Acceptance (Windows 11 VM; checked by hand 2026-10-29 with the 0.1.7 installer)
- [x] Install → first run → Default apps page opens with Crate listed
- [x] Extracting an archive containing `CON.txt` and a 300-character path shows the rename prompt and succeeds
- [x] Files extracted from a downloaded zip keep the "downloaded from the internet" flag

## Notes

### Build and CI on Windows (2026-10-28, 0.1.2)
- `ci.yml` has a `windows` job on `windows-latest`: clippy and the whole test suite (the UI flows too, with Slint's software renderer), and keeps `target/ui-snapshots/` as an artifact, so the Windows rendering can be looked at without a VM. `cargo fmt` only runs on Ubuntu.
- `.gitattributes` keeps every text file LF on every system (the Windows runner checks out CRLF otherwise, and tests compare exact bytes); archives and images are binary.
- Most platform code was already behind `cfg(unix)` stubs. The one real gap was RAR: unrar_sys takes the archive name as UTF-16 on Windows (`OpenArchiveDataEx::new` has a different signature there), and passwords and located volumes go to UnRAR as UTF-16 too, so names outside the Basic Multilingual Plane survive.
- What the first Windows runs turned up:
  - **Editing an open archive failed** ("Access is denied"): tempfile's `persist` (MoveFileEx) can't replace a file another handle has open, and the window keeps its archive open. `AtomicFile::commit` now uses `fs::rename`, which uses POSIX semantics on Windows 10+ and replaces it (Rust opens files with delete sharing).
  - **No `HOME`:** `app/src/dirs.rs` puts settings in `%APPDATA%\crate`, history and cache in `%LOCALAPPDATA%\crate`; the XDG variables still win when set (the tests rely on that). Downloads and Desktop come from the shell's known folders, which follow OneDrive redirection.
  - **Paths read as on Windows:** full paths with `\` (`C:\Users\you\Downloads\project-assets\`, as in `MiniView.dc.html`), no `~`.
  - **Symlinks** need Developer Mode or administrator rights to create, so a link to a file extracted in the same run becomes a copy of that file (as hard links already do everywhere); links to folders are skipped as "not supported on this system".
  - The tests now compare paths with `/` on every system; `cargo test --no-fail-fast` on Windows shows every failing binary in one run, and the cache is kept even when a run fails (Windows builds take about 25 minutes cold).

### File names (2026-10-28, 0.1.3)
- `crate_core::names`: `NameRules` is a value (Windows rules, case-insensitive), not a `cfg`, so the Windows rules run in tests everywhere; `NameRules::native()` is the default in `ExtractOptions`. `fix_component` follows the mockup: `CON.txt` → `CON_.txt` (CON, PRN, AUX, NUL, COM1–9, LPT1–9, with ¹²³), forbidden characters and control characters → `_`, a trailing dot or space is dropped.
- Renaming happens in the extraction plan, before anything is written: a folder that's renamed takes everything inside along. `ExtractOptions` gained `skip_bad_names`, `edited_names` (typed by the user, keyed by the original path; checked to be one plain component, so a typed `../x` is ignored), `case_clash` and `case_choices`.
- Case clashes (two files whose paths differ only by case, on Windows and macOS): Keep both (the later one becomes `readme (2).md`, the default), Keep newer only (one file, the newer contents, under the first spelling), or Skip the later one.
- Long paths aren't renamed: Rust's standard library switches to `\\?\` paths by itself past 260 characters. They're listed in the prompt ("uses long-path mode") so people know why Explorer may struggle with them.
- `Archive::check_names` gives the app the list up front: the app asks with the two dialogs from the mockup (`app/ui/name-dialogs.slint`), first the renames (Skip these / Edit names… / Rename and extract), then each case clash (with "Apply to all"); answers go into the request's options, and the check runs again after the first dialog because edits can change the clashes. Batch extract and the CLI apply the defaults without asking; the CLI prints what it renamed.
- `CRATE_NAME_RULES=windows` turns the Windows rules on anywhere (`app/tests/names_flow.rs`, screenshots 130–134, and for trying the prompt on Linux).

### Mark-of-the-Web (2026-10-28, 0.1.4)
- `crate_core::zone`: reads the archive's `Zone.Identifier` stream and writes the same bytes onto every extracted file (and hard-link copies) while they're still in the staging folder; the stream moves with them into place. Like Explorer, every file gets it, whatever its type. A file system without alternate streams (FAT32, some shares) just goes without; nothing fails. A no-op on Linux and macOS (macOS has its own quarantine attribute: M8).
- Archives inside the archive pass the mark on: the inner archive got it when extracted, so unpacking it copies it again.
- Tested on the Windows runner only (`crates/crate-core/tests/zone.rs`): a marked zip gives marked files, including one with an Arabic name; an unmarked zip gives none.

### Windows platform module: taskbar progress, default apps (2026-10-28, 0.1.5)
- `app/src/platform/windows.rs` (declared as `mod win`: `windows` is the crate's name). `other.rs` is now macOS only.
- Taskbar progress: `ITaskbarList3` (the `windows` crate, already in the tree through Slint) on the window's HWND from winit. `View::progress` sets it, and every place that ends a busy state clears it. Linux has no common way to do this (the Unity launcher API only works on some docks), so it's a no-op there.
- Default apps: Windows doesn't let apps make themselves the default, so `Defaults::SettingsPage` (4) means that "make Crate the default" opens `ms-settings:defaultapps?registeredAppUser=Crate`, Crate's own page in Settings → Default apps. That name is the one the installer registers under `RegisteredApplications`; without it Crate counts as not installed (`Defaults::NotInstalled`). Preferences reads the user's choice per type from `…\Explorer\FileExts\<ext>\UserChoice` (ProgId `Crate.Archive`). First run shows a note and opens the page on Get started; the type chips are only shown where Crate sets the defaults itself (Linux).
- `crate-app.exe` gets the icon and version info from `app/build.rs` (`winresource`, Windows targets only). The release build already has no console window (`windows_subsystem`).

### Installer, file associations, Explorer menu (2026-10-28, 0.1.6)
- **Inno Setup 6** (`packaging/windows/crate.iss`, built by `scripts/package-windows.ps1`). WiX was the other candidate. Inno is free for any use, including with the Pro tier. It installs per user without administrator rights (`PrivilegesRequired=lowest`), and per machine on request (first page, or `/ALLUSERS`). winget supports it. The installer is `Crate-<version>-x64-setup.exe`.
- What it registers, under HKCU or HKLM to match the scope:
  - The ProgID `Crate.Archive`.
  - `OpenWithProgids` for .zip .7z .rar .tar .tgz .gz .txz .xz .tzst .zst .tbz2 .bz2 .001, so Crate is in "Open with".
  - `Applications\crate-app.exe`.
  - `Software\Crate\Capabilities` and `RegisteredApplications` → "Crate", so Crate is in Settings → Default apps.
  - All of it goes again on uninstall.
- Explorer: classic verbs under `SystemFileAssociations\<ext>\shell` ("Open with Crate", "Extract with Crate" = `--extract-each`, Batch extract into a folder next to the archive), and "Compress with Crate…" (`--compress`) on any file or folder. On Windows 11 they're under "Show more options"; the modern menu needs an IExplorerCommand handler and package identity (post-v1). Classic verbs start one process per selected item, so compressing three selected files opens three dialogs: a known limit until then.
- `release.yml` is now three jobs: `linux`, `windows` (Inno Setup from Chocolatey, tests, installer, then a silent per-user install that checks the registry entries and the CLI, and an uninstall that must leave nothing behind) and `publish` (on tags: merges both `SHA256SUMS`, checks them, CHANGELOG notes, the GitHub Release).
- README: the Windows row and a "Windows" section (no admin rights needed, SmartScreen's "More info → Run anyway" for the unsigned installer, checking the hash with `Get-FileHash`, default apps).

### winget (2026-10-28, 0.1.7)
- `scripts/winget-manifest.py <version>` writes the three manifests (version, installer, en-US locale; schema 1.10.0) for package id `mlbrainlab.Crate` into `target/winget/…`. It reads the installer's SHA-256 from the published release's `SHA256SUMS`. One Inno installer serves both scopes (`/CURRENTUSER`, `/ALLUSERS`).
- The submission is a pull request to microsoft/winget-pkgs from the user's account, once 0.2.0 is published (`docs/winget.md`), as with Flathub. The box is ticked for the manifest; "the winget manifest is submitted" in Done-when waits for that.

### Window frame (2026-10-29)
- Windows keeps the native title bar (`CUSTOM_FRAME = false`): Snap Layouts, the system menu, dark/light and accent colors, Windows 11 corners and accessibility come with it, and it's what `DESIGN.md` and the mockups show. The user decided it after comparing both.
- A frameless window would need Snap Layouts done by hand: Windows only shows them when `WM_NCHITTEST` answers `HTMAXBUTTON` over the maximize button, and winit has no hook for that, so it means subclassing the window procedure. Moved to M9, where the Glass theme draws its own frame anyway.

### 0.2.0 (2026-10-29)
- Every task and the acceptance checks are done; the installer from the release dry run was installed and checked by hand on a Windows 11 VM.
- Left for "Done when": publishing 0.2.0 (tag `v0.2.0`), then the winget pull request from the user's account (`docs/winget.md`).

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M7-windows.md. Keep Windows-only code behind `#[cfg(windows)]` in a `platform` module.
