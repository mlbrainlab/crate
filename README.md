# Crate

A fast, cross-platform archiver: open, browse, extract and create archives. Written in Rust: a Slint UI on top of a pure-Rust archive engine.

> **Status:** early development (milestone M0, see [docs/ROADMAP.md](docs/ROADMAP.md)). Nothing to download yet.

## Planned
- Double-click an archive → a small **mini view** that extracts in one click
- A full view to browse, preview, add, rename and delete files inside archives
- ZIP, TAR (gz / xz / zst / bz2), 7z, and RAR (extract only)
- Safe by default: blocks path-escape ("zip-slip") and symlink tricks, warns about decompression bombs
- Linux first, then Windows and macOS

## Build from source (Ubuntu)
Prerequisites: Rust (stable, via rustup) and Slint's build dependencies:

```
sudo apt install build-essential libfontconfig1-dev libxkbcommon-dev libwayland-dev
```

```
cargo run -p crate-app              # run the app (add `-- --mini` for the mini view)
cargo test --workspace              # tests
cargo run -p crate-cli -- --help    # command-line tool
```

## Windows builds
Windows builds will ship unsigned, so SmartScreen will warn on first run ("More info → Run anyway"). Every release will publish checksums.

## Support
Crate is free and open source. If it helps you, see the Sponsor button on this repo.

## License
[MIT](LICENSE)
