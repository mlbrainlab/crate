# Crate: dev workflow

## One codebase
- One repo, one codebase for all platforms. Platform-only code (about 5–10%) lives behind `#[cfg(...)]` in `app/src-tauri/src/platform/{linux,windows,macos}.rs`.
- GitHub is the single source of truth. Every machine clones it; nothing is copied around by hand.

## Machines
| Machine | Role | Setup |
|---|---|---|
| Ubuntu (host) | Main dev + Linux builds (M0–M6) | Rust, Node LTS, pnpm, Tauri Linux packages, Claude Code |
| Windows 11 VM | Windows builds + testing (M7) | Rust (MSVC toolchain), Visual Studio Build Tools "Desktop development with C++", Node, pnpm, Git. **Clone the repo inside the VM**; don't build from a shared folder (slow, and file watching breaks) |
| Mac mini | macOS builds, signing, notarization (M8) | Xcode Command Line Tools, Rust (+ `x86_64-apple-darwin` target for universal builds), Node, pnpm, Apple Developer certificate |

- Tauri builds each OS **on that OS**. Don't count on cross-compiling.
- Daily loop: write and test on Ubuntu → push → `git pull` on the VM or Mac only when working on that platform.
- Releases: GitHub Actions builds all three on version tags (`ubuntu-latest`, `windows-latest`, `macos-latest`). The VM and Mac are for debugging and hands-on testing, not for release builds.

## Parallel Claude Code sessions
- Split work by **area**, not by platform: e.g. session A on `crate-core` (formats + tests), session B on the UI (mini view).
- Give each session its own git worktree + branch, and merge through PRs. Two sessions must never edit the same files.
- Run at most **2 at a time**; more creates merge conflicts and too much context-switching.
- Start M0 with one session. Parallel work starts at M1 (core) + M2 (UI), which barely overlap.

## Agents that help (no AI inside the app)
- **Reviewer:** before merging, a fresh session reviews the diff against CLAUDE.md rules (safety module used, no in-place writes, color tokens only).
- **Fixture / fuzz helper:** generates malicious and edge-case archives for `tests/fixtures/` and grows the fuzz corpus.
- **Release helper:** drafts CHANGELOG entries and release notes from merged PRs.
- **Issue triage** (after v0.1): label and reproduce incoming GitHub issues.
