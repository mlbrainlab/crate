# M6: Linux release v0.1

**Goal:** the first public release on GitHub, installable on most Linux distros.
**Done when:** v0.1.0 is on GitHub Releases and the Flathub submission is sent.

## Decision to make first
- [x] App icon: concept #1 "Open Lid". The SVG masters, PNG 16–1024, .ico and .icns are in `assets/icons/`

## Tasks
- [x] Packaging: AppImage, .deb, .rpm (candidates: `cargo-packager` + `cargo-generate-rpm`; pick here). `.desktop` file named after the app id `io.github.mlbrainlab.crate`, hicolor icons from `assets/icons/png`
- [ ] Flatpak manifest + AppStream metainfo (description, screenshots, release notes); submit to Flathub
- [ ] GitHub Actions release workflow, triggered by version tags; checksums file attached to each release
- [x] README per `Readme.dc.html`: screenshots, install table, how it's built, benchmarks (real numbers or remove the section), Support
- Moved to M9 (2026-10-28): the README note about blur for the Glass theme, which ships with the theme.
- [x] `CHANGELOG.md`, issue templates, CONTRIBUTING
- [x] Fuzz the archive readers with `cargo-fuzz` (at least ZIP and tar), with a short run in CI
- [ ] Test on a clean Ubuntu VM + one other distro (Fedora)

## Notes

### Packaging (2026-10-28, 0.0.6)
- Picked **cargo-deb** (.deb), **cargo-generate-rpm** (.rpm) and **appimagetool** (AppImage): mature, driven by `[package.metadata.deb]` / `[package.metadata.generate-rpm]` in `app/Cargo.toml`. `scripts/package-linux.sh` builds all three into `target/dist/` with `SHA256SUMS`; it writes the installed `.desktop` (Exec=`crate-app`) to `target/packaging/` first and validates it and the AppStream file (`packaging/linux/io.github.mlbrainlab.crate.metainfo.xml`).
- Package name **crate-archiver** (plain "crate" is too generic for distro repos). It installs `crate-app` and the `crate` CLI, the `.desktop` entry, hicolor icons 16–512 + scalable SVG, and the metainfo. The AppImage runs the app; `--cli …` runs the CLI.
- **Built on Ubuntu 22.04** (glibc 2.35): built on this 26.04 machine the .deb needed glibc 2.43. `packaging/linux/Dockerfile.build` gives the same environment locally; the release workflow uses an `ubuntu-22.04` runner.
- Dependencies: winit loads X11, xkbcommon, Wayland and EGL/GL at run time, so they're listed by hand (dpkg-shlibdeps and rpm can't see them). fontconfig isn't bundled in the AppImage (on purpose, per AppImage's exclude list; every desktop has it).
- Checked in containers, each with a real window under Xvfb (`--startup-probe` opens an archive and exits when its listing is on screen): the .deb on Ubuntu 24.04 (apt resolves everything), the .rpm on Fedora 44 (dnf), the AppImage on Fedora 44 with only the desktop libraries. The CLI creates, lists and extracts in each.
- `.github/workflows/release.yml`: a `v*` tag checks the tag against the version, runs the tests, builds the packages on ubuntu-22.04 and publishes a GitHub Release with them, `SHA256SUMS` and the CHANGELOG section as notes. Run by hand (workflow_dispatch) it's a dry run: the packages become an artifact, nothing is published.

### README (2026-10-28, 0.0.7)
- Follows `Readme.dc.html` where it's true today: Linux-only install table (Windows / macOS marked planned, Flathub "coming"), no Flathub badge or ISO claim, no donation links (no accounts yet; `FUNDING.yml` is still placeholders, so GitHub shows no Sponsor button either).
- Screenshots: `app/tests/readme_shots.rs` renders them with tidy content (a stand-in home folder, so paths read `~/Downloads/…`); `scripts/readme-screenshots.sh` copies them to `docs/screenshots/`, which the AppStream file also points at.
- Benchmarks are real: `scripts/bench-cli.sh` generates the data and runs hyperfine (crate x vs unzip, crate c vs tar + zstd -T0, crate l vs tar -t). Two things came out of it: tar.zst creation now uses zstd's worker threads (`zstdmt`; 4.2 s → 2.0 s on 1 GB), and the comparison syncs tar's output too, because Crate syncs a new archive to disk before reporting success (so "Move originals to Trash" is safe). Listing is 2.6× slower than `tar -t`: left as it is, said in the README.
- The New archive dialog's Output line now shows `~/…` like Save in (`model::display_path`).
- Not done: the blur note for the Glass theme waits for the theme itself (M9).

### CHANGELOG, issue templates, CONTRIBUTING (2026-10-28, 0.0.8)
- `CHANGELOG.md` has existed since 0.0.2 (one entry per version; the release workflow takes a version's section as the release notes).
- `.github/ISSUE_TEMPLATE/`: bug report (version, system, what happened, the archive), feature request; blank issues off; security problems go to private vulnerability reporting (needs "Private vulnerability reporting" switched on in the repo's Settings → Security).
- CONTRIBUTING.md rewritten for outside contributors: reporting bugs, the layout of the repo, checks, pull-request rules (the project's working rules), translations.
- `crate-app --version` (the bug template asks for it).

### Fuzzing (2026-10-28, 0.0.9)
- `crates/crate-core/fuzz/` (its own workspace; nightly): targets `zip`, `tar`, `tar_gz`, `sevenz`. Each writes the input to a temp file and runs open → list → test → extract with small limits (100× ratio past 1 MB), then asserts the safety promise: nothing written outside the destination, no link inside it pointing out. RAR is left out (UnRAR is C++ that libFuzzer doesn't instrument; build without the `rar` feature).
- Seeds: `scripts/fuzz-seeds.sh` makes small archives with the CLI (folders, a symlink) into `fuzz/corpus/<target>/`; libFuzzer's discoveries go elsewhere and aren't committed.
- First run, 60 s per target on this machine: 1.8 million inputs, 3,959 new code paths found, no crash, no escape.
- CI: a `fuzz` job runs each target for 30 s on every push and keeps any crashing input as an artifact.

### Flatpak (2026-10-28)
- `packaging/flatpak/io.github.mlbrainlab.crate.yml`: runtime 25.08 + rust-stable, offline build from `cargo-sources.json` (707 crates). The generator is our own `scripts/flatpak-cargo-sources.py` (the same format as flatpak-builder-tools' generator, crates.io only). Permissions: Wayland / fallback X11, GPU, `--filesystem=host` (an archive manager extracts next to the archive; the portal grants only the picked file), notifications.
- Built and run locally with `org.flatpak.Builder` (needs `--disable-rofiles-fuse` when the Builder itself is a Flatpak): the CLI creates and extracts, the app opens an archive in a real window in 107 ms.
- Under Flatpak every `flatpak run` has its own runtime dir and PIDs, so the Operations registry lives in the app's shared runtime folder and tells live processes by a held lock file.
- `flatpak-builder-lint`: the only errors are about unreachable URLs (homepage, screenshots), because **the GitHub repository is private**. It has to be public before the release and the Flathub submission (`docs/flathub.md` has the steps; the user submits from their account).

## Acceptance
- [ ] Fresh Ubuntu: download the .deb from Releases → install → double-click a zip → works
- [ ] AppImage runs on Fedora without installing anything else
- [ ] CI builds every package from a tag with no manual steps

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M6-linux-release.md. Set up packaging and the release workflow first, then the README, then fuzzing.
