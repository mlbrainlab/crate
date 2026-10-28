# M7: Windows port

**Goal:** Crate on Windows 10/11 with the same features. Shipped unsigned.
**Done when:** the .msi/.exe from GitHub Releases installs and works on Windows 11, and the winget manifest is submitted.

## Tasks
- [x] Build and CI on `windows-latest`
- [ ] Window frame: Crate's own header + caption buttons on the right; check whether Snap Layouts can be kept with a frameless winit window (hit-testing the maximize button)
- [x] File names: reserved names, illegal characters, long paths, names that differ only by case (`FileIssues.dc.html`)
- [x] Copy the "downloaded from the internet" flag (Mark-of-the-Web, `Zone.Identifier`) onto extracted files
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

## Notes

### Build and CI on Windows (2026-10-28, 0.1.2)
- `ci.yml` has a `windows` job on `windows-latest`: clippy and the whole test suite (the UI flows too, with Slint's software renderer), and keeps `target/ui-snapshots/` as an artifact, so the Windows rendering can be looked at without a VM. `cargo fmt` only runs on Ubuntu.
- `.gitattributes` keeps every text file LF on every system (the Windows runner checks out CRLF otherwise, and tests compare exact bytes); archives and images are binary.
- Most platform code was already behind `cfg(unix)` stubs. The one real gap was RAR: unrar_sys takes the archive name as UTF-16 on Windows (`OpenArchiveDataEx::new` has a different signature there), and passwords and located volumes go to UnRAR as UTF-16 too, so names outside the Basic Multilingual Plane survive.

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

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M7-windows.md. Keep Windows-only code behind `#[cfg(windows)]` in a `platform` module.
