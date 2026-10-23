# Crate

A fast, cross-platform archiver: open, browse, extract and create archives. Built with Tauri 2 and a pure-Rust archive engine.

> **Status:** early development (milestone M0, see [docs/ROADMAP.md](docs/ROADMAP.md)). Nothing to download yet.

## Planned
- Double-click an archive → a small **mini view** that extracts in one click
- A full view to browse, preview, add, rename and delete files inside archives
- ZIP, TAR (gz / xz / zst / bz2), 7z, and RAR (extract only)
- Safe by default: blocks path-escape ("zip-slip") and symlink tricks, warns about decompression bombs
- Linux first, then Windows and macOS

## Build from source (Ubuntu)
Prerequisites: Rust (stable, via rustup), Node LTS, pnpm, and the [Tauri Linux packages](https://v2.tauri.app/start/prerequisites/#linux).

```
pnpm install
pnpm tauri dev                      # run the app
cargo test --workspace              # Rust tests
cargo run -p crate-cli -- --help    # command-line tool
```

## Windows builds
Windows builds will ship unsigned, so SmartScreen will warn on first run ("More info → Run anyway"). Every release will publish checksums.

## Support
Crate is free and open source. If it helps you, see the Sponsor button on this repo.

## License
[MIT](LICENSE)
