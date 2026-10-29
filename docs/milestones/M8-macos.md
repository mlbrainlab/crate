# M8: macOS port

**Goal:** Crate on macOS 12+ (Apple Silicon + Intel), signed and notarized.
**Done when:** the universal .dmg installs with no Gatekeeper warning and the Homebrew cask is submitted.

## Tasks
- [x] Build and CI on `macos-latest` (universal binary)
- [ ] Window: native traffic lights placed inside Crate's own header (overlay title bar through winit's macOS window attributes in `platform::select_backend`); a native menu bar (File / Edit / Archive / View / Window / Help), using Slint's `MenuBar` if it maps to the native macOS menu
- [ ] Document types in the app bundle's Info.plist (file associations) so Crate appears in "Open With"
- [ ] Default app: first run calls the macOS API for setting the default app per file type (small native code); the system may show its own confirmation. Verify on the current macOS version
- [ ] Ask for folder access when needed (`FileIssues.dc.html`, permission card)
- [ ] Code signing + notarization in CI (Apple Developer account ✔); store the secrets in GitHub
- [ ] Dock progress
- [ ] Homebrew cask

## Acceptance (macOS)
- [ ] Download the .dmg → open → no "unidentified developer" warning
- [ ] First run → make default → double-clicking a .zip opens the mini view

## Notes

### Build, CI and the bundle (2026-10-29, 0.2.1)
- `ci.yml` has a `macos` job (clippy and every test on `macos-latest`, Apple Silicon). It passed the first time: nothing in the code was Linux- or Windows-only without a fallback. The RAR reader already passes wide characters as `wchar_t` (32-bit on macOS).
- `scripts/package-macos.sh` builds both targets with `MACOSX_DEPLOYMENT_TARGET=12.0`, joins them with `lipo`, and lays out `Crate.app`: `Contents/MacOS/crate-app` (the bundle's executable) and `crate` (the CLI, for the Homebrew cask to link), `Info.plist` from `packaging/macos/Info.plist`, `crate.icns`, `LICENSE`. Then the `.dmg` (the app and a link to /Applications), `Crate-<version>-universal.dmg`, and `SHA256SUMS`.
- Signing: `MACOS_SIGN_IDENTITY` signs with the hardened runtime and a timestamp; without it the app is signed ad hoc (Apple Silicon won't run an unsigned binary at all). Notarization comes with the signing task.
- `release.yml` has a `macos` job: tests, the `.dmg`, then it mounts it, checks the signature, runs the CLI natively and under Rosetta, and uploads it as an artifact. `publish` leaves it out until it's signed and notarized.
- Dry run 40931301588 (2026-10-29): every job green; `Crate-0.2.1-universal.dmg` mounted, signature valid, `crate 0.2.1` on arm64 and under Rosetta.
- `Info.plist` already declares the document types (task 3): the Windows installer's extensions. Types macOS knows (zip, tar, gz, bz2) rank "Alternate", so Crate is in "Open With" without taking them from Archive Utility; 7z, rar, xz, zst, tzst, tbz2, txz and .001 are declared (`UTImportedTypeDeclarations`) and rank "Default".

### Opening files from Finder (2026-10-29)
- Finder doesn't pass the file as an argument: it launches the app (or finds it running) and sends an "open documents" Apple Event, which AppKit hands to the app delegate's `application:openURLs:`. winit 0.30 has neither the method nor an event for it.
- `platform/macos.rs` adds the method to winit's delegate class (`class_addMethod`, as winit does for `sendEvent:`) right after `select_backend()` made the event loop, then sets the delegate again so AppKit sees it. Files that arrive before anyone asks are queued (`platform::take_opened_files`); after that `platform::on_open_files` gets them. Other systems have no-op versions.
- `main.rs`: launched with no arguments on macOS (`run_launched`), Crate decides on the event loop's first turn, after the launch event: the first archive opens in the mini view in this process (with the usual one-window-per-archive claim), any others in new processes, and with none, Home. Files sent to a running Crate open in new processes (`open_each`), as Home's Open does.
- **To check on the Mac:** double-click a .zip with Crate as its app (mini view, no Home flash); a second archive while it's open; the same archive twice (the first window comes to the front); "Open With → Crate" for each type.

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M8-macos.md. Keep macOS-only code behind `#[cfg(target_os = "macos")]`.
