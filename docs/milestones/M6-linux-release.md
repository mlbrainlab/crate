# M6: Linux release v0.1

**Goal:** the first public release on GitHub, installable on most Linux distros.
**Done when:** v0.1.0 is on GitHub Releases and the Flathub submission is sent.

## Decision to make first
- [x] App icon: concept #1 "Open Lid". The SVG masters, PNG 16–1024, .ico and .icns are in `assets/icons/`

## Tasks
- [x] Packaging: AppImage, .deb, .rpm (candidates: `cargo-packager` + `cargo-generate-rpm`; pick here). `.desktop` file named after the app id `io.github.mlbrainlab.crate`, hicolor icons from `assets/icons/png`
- [ ] Flatpak manifest + AppStream metainfo (description, screenshots, release notes); submit to Flathub
- [ ] GitHub Actions release workflow, triggered by version tags; checksums file attached to each release
- [ ] README per `Readme.dc.html`: screenshots, install table, how it's built, benchmarks (real numbers or remove the section), Support
- [ ] README note for blur on Linux: "For the Glass theme's see-through blur on GNOME, install a blur extension such as Blur my Shell; KDE Plasma blurs natively." Test the exact extension name and setting yourself before publishing
- [ ] `CHANGELOG.md`, issue templates, CONTRIBUTING
- [ ] Fuzz the archive readers with `cargo-fuzz` (at least ZIP and tar), with a short run in CI
- [ ] Test on a clean Ubuntu VM + one other distro (Fedora)

## Notes

### Packaging (2026-10-28, 0.0.6)
- Picked **cargo-deb** (.deb), **cargo-generate-rpm** (.rpm) and **appimagetool** (AppImage): mature, driven by `[package.metadata.deb]` / `[package.metadata.generate-rpm]` in `app/Cargo.toml`. `scripts/package-linux.sh` builds all three into `target/dist/` with `SHA256SUMS`; it writes the installed `.desktop` (Exec=`crate-app`) to `target/packaging/` first and validates it and the AppStream file (`packaging/linux/io.github.mlbrainlab.crate.metainfo.xml`).
- Package name **crate-archiver** (plain "crate" is too generic for distro repos). It installs `crate-app` and the `crate` CLI, the `.desktop` entry, hicolor icons 16–512 + scalable SVG, and the metainfo. The AppImage runs the app; `--cli …` runs the CLI.
- **Built on Ubuntu 22.04** (glibc 2.35): built on this 26.04 machine the .deb needed glibc 2.43. `packaging/linux/Dockerfile.build` gives the same environment locally; the release workflow uses an `ubuntu-22.04` runner.
- Dependencies: winit loads X11, xkbcommon, Wayland and EGL/GL at run time, so they're listed by hand (dpkg-shlibdeps and rpm can't see them). fontconfig isn't bundled in the AppImage (on purpose, per AppImage's exclude list; every desktop has it).
- Checked in containers, each with a real window under Xvfb (`--startup-probe` opens an archive and exits when its listing is on screen): the .deb on Ubuntu 24.04 (apt resolves everything), the .rpm on Fedora 44 (dnf), the AppImage on Fedora 44 with only the desktop libraries. The CLI creates, lists and extracts in each.
- `.github/workflows/release.yml`: a `v*` tag checks the tag against the version, runs the tests, builds the packages on ubuntu-22.04 and publishes a GitHub Release with them, `SHA256SUMS` and the CHANGELOG section as notes. Run by hand (workflow_dispatch) it's a dry run: the packages become an artifact, nothing is published.

## Acceptance
- [ ] Fresh Ubuntu: download the .deb from Releases → install → double-click a zip → works
- [ ] AppImage runs on Fedora without installing anything else
- [ ] CI builds every package from a tag with no manual steps

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M6-linux-release.md. Set up packaging and the release workflow first, then the README, then fuzzing.
