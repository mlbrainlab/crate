# M8: macOS port

**Goal:** Crate on macOS 12+ (Apple Silicon + Intel), signed and notarized.
**Done when:** the universal .dmg installs with no Gatekeeper warning and the Homebrew cask is submitted.

## Tasks
- [ ] Build and CI on `macos-latest` (universal binary)
- [ ] Window: native traffic lights placed inside Crate's own header (overlay title bar); a native menu bar (File / Edit / Archive / View / Window / Help)
- [ ] Document types in the bundle (file associations config) so Crate appears in "Open With"
- [ ] Default app: first run calls the macOS API for setting the default app per file type (small native code); the system may show its own confirmation. Verify on the current macOS version
- [ ] Ask for folder access when needed (`FileIssues.dc.html`, permission card)
- [ ] Code signing + notarization in CI (Apple Developer account ✔); store the secrets in GitHub
- [ ] Dock progress
- [ ] Homebrew cask

## Acceptance (macOS)
- [ ] Download the .dmg → open → no "unidentified developer" warning
- [ ] First run → make default → double-clicking a .zip opens the mini view

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M8-macos.md. Keep macOS-only code behind `#[cfg(target_os = "macos")]`.
