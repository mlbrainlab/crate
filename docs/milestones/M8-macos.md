# M8: macOS port

**Goal:** Crate on macOS 12+ (Apple Silicon + Intel), signed and notarized.
**Done when:** the universal .dmg installs with no Gatekeeper warning and the Homebrew cask is submitted.

## Tasks
- [ ] Build and CI on `macos-latest` (universal binary)
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
- `Info.plist` already declares the document types (task 3): the Windows installer's extensions. Types macOS knows (zip, tar, gz, bz2) rank "Alternate", so Crate is in "Open With" without taking them from Archive Utility; 7z, rar, xz, zst, tzst, tbz2, txz and .001 are declared (`UTImportedTypeDeclarations`) and rank "Default".
- **Still to do for task 3:** Finder doesn't pass the file as a command-line argument; it launches the app and sends an "open documents" Apple Event (`application:openURLs:` on the app delegate). winit 0.30 has no event for it, so Crate opens its home window instead of the archive. Needs native code: add the method to winit's delegate class with `objc2`, and hand the URLs to the same path as `argv` (including the single-instance hand-over for a second file).

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M8-macos.md. Keep macOS-only code behind `#[cfg(target_os = "macos")]`.
