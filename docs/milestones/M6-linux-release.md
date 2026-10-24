# M6: Linux release v0.1

**Goal:** the first public release on GitHub, installable on most Linux distros.
**Done when:** v0.1.0 is on GitHub Releases and the Flathub submission is sent.

## Decision to make first
- [x] App icon: concept #1 "Open Lid". The SVG masters, PNG 16–1024, .ico and .icns are in `assets/icons/`

## Tasks
- [ ] Packaging: AppImage, .deb, .rpm (candidates: `cargo-packager` + `cargo-generate-rpm`; pick here). `.desktop` file named after the app id `io.github.mlbrainlab.crate`, hicolor icons from `assets/icons/png`
- [ ] Flatpak manifest + AppStream metainfo (description, screenshots, release notes); submit to Flathub
- [ ] GitHub Actions release workflow, triggered by version tags; checksums file attached to each release
- [ ] README per `Readme.dc.html`: screenshots, install table, how it's built, benchmarks (real numbers or remove the section), Support
- [ ] README note for blur on Linux: "For the Glass theme's see-through blur on GNOME, install a blur extension such as Blur my Shell; KDE Plasma blurs natively." Test the exact extension name and setting yourself before publishing
- [ ] `CHANGELOG.md`, issue templates, CONTRIBUTING
- [ ] Fuzz the archive readers with `cargo-fuzz` (at least ZIP and tar), with a short run in CI
- [ ] Test on a clean Ubuntu VM + one other distro (Fedora)

## Acceptance
- [ ] Fresh Ubuntu: download the .deb from Releases → install → double-click a zip → works
- [ ] AppImage runs on Fedora without installing anything else
- [ ] CI builds every package from a tag with no manual steps

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M6-linux-release.md. Set up packaging and the release workflow first, then the README, then fuzzing.
