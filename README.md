# Crate

A fast, cross-platform archiver: open, browse, extract and create archives. Written in Rust: a Slint UI on top of a pure-Rust archive engine.

> **Status:** early development (milestone M3, see [docs/ROADMAP.md](docs/ROADMAP.md)). Nothing to download yet.

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
cargo run -p crate-app -- <archive>  # open an archive in the mini view
cargo run -p crate-app               # the full window (empty until M3)
cargo test --workspace               # tests
cargo run -p crate-cli -- --help     # command-line tool
```

To open archives from Files ("Open With → Crate"), install a desktop entry that points at this checkout's release build:

```
scripts/dev-install-desktop.sh       # --uninstall removes it again
```

## Windows builds
Windows builds will ship unsigned, so SmartScreen will warn on first run ("More info → Run anyway"). Every release will publish checksums.

## Support
Crate is free and open source. If it helps you, see the Sponsor button on this repo.

## License
Crate is [MIT](LICENSE) licensed.

RAR extraction uses the UnRAR library by Alexander Roshal (through the `unrar_sys` crate). UnRAR is freeware, not open source. Its license lets any software use it to extract RAR archives free of charge, but it may not be used to create a RAR-compatible archiver or to recreate the RAR compression algorithm. Crate only reads RAR. The full terms ship with the source of `unrar_sys` (`vendor/unrar/license.txt`). To build without UnRAR, for example for a distribution that only accepts free software, use `cargo build --no-default-features`; RAR files then report "not supported".
