# M1: Core: list & extract safely

**Goal:** `crate-core` can list and extract the main formats safely, and the CLI exposes it.
**Done when:** the CLI extracts real archives correctly, and every malicious test archive is blocked.

## Tasks
### Formats (read)
- [x] ZIP (incl. ZIP64), tar, tar.gz, tar.xz, tar.zst, tar.bz2
- [x] 7z (read)
- [x] RAR (read, `unrar` crate; check its license terms and note them in the README) → uses `unrar_sys` directly to stream data; UnRAR terms in README; `rar` cargo feature (default on)
- [x] One `Archive` trait: `list() -> entries`, `extract(selection, dest, options, progress, cancel)` → built as the `Archive` type (`entries()`, `extract()`, `test()`) over a per-format `FormatReader` trait, so every format shares one extraction path

### Safety (`crate_core::safety`)
- [x] Reject entries that resolve outside the destination (`../`, absolute paths, drive letters)
- [x] Symlinks: never follow a link that points outside the destination
- [x] Decompression-bomb check: total unpacked size vs free disk space, plus a compression-ratio limit (configurable) → default 1000× above 64 MiB; declared sizes also enforced while writing
- [x] Write to a temp folder next to the destination, move into place on success, delete it on cancel or error
- [x] Return a report of skipped items (shown later as "2 items were skipped for your safety")

### Filename encoding
- [x] Detect non-UTF-8 ZIP names; guess with `chardetng`; allow an override (CP1256, CP437, Shift-JIS, GBK, CP1251)

### Progress & cancel
- [x] Progress callback: bytes done, files done, current path; cancel token checked between files and chunks

### CLI
- [x] `crate l <archive>`, `crate x <archive> [-o dir] [--encoding cp1256]`, `crate t <archive>` (test integrity)

### Test fixtures (`tests/fixtures/`)
- [x] Normal: zip, tar.zst, 7z, nested folders, unicode names
- [x] Malicious: path traversal, absolute path, symlink escape, decompression bomb (small file, huge expansion)
- [x] Old-encoding ZIP with Arabic CP1256 names

## Notes
- RAR can't be generated for tests (no free RAR writer); the RAR tests use sample files from the `unrar` crate plus damaged copies. Path and link safety is format-independent and tested with ZIP, tar and 7z.
- Passwords (encrypted entries are skipped and reported for now) come with M2's password prompt.
- Multi-volume archives (`.part2.rar`, split 7z/zip) are for M5 (`EdgeCases.dc.html`).

## Design references
- `EdgeCases.dc.html` (safety report, bomb warning) and `MiniViewRTL.dc.html` (encoding)

## Acceptance (on Ubuntu)
- [x] `cargo test --workspace` passes, including every malicious fixture
- [x] `crate x tests/fixtures/arabic-cp1256.zip --encoding cp1256` produces readable Arabic names
- [x] Ctrl+C during a large extract leaves no partial files behind

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M1-core-extract.md. Build crate-core and crate-cli step by step: one format plus its tests per commit, starting with ZIP and the safety module. Generate the malicious fixtures with a small Rust test helper rather than downloading them.
