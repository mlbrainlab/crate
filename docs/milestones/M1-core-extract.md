# M1: Core: list & extract safely

**Goal:** `crate-core` can list and extract the main formats safely, and the CLI exposes it.
**Done when:** the CLI extracts real archives correctly, and every malicious test archive is blocked.

## Tasks
### Formats (read)
- [ ] ZIP (incl. ZIP64), tar, tar.gz, tar.xz, tar.zst, tar.bz2
- [ ] 7z (read)
- [ ] RAR (read, `unrar` crate; check its license terms and note them in the README)
- [ ] One `Archive` trait: `list() -> entries`, `extract(selection, dest, options, progress, cancel)`

### Safety (`crate_core::safety`)
- [ ] Reject entries that resolve outside the destination (`../`, absolute paths, drive letters)
- [ ] Symlinks: never follow a link that points outside the destination
- [ ] Decompression-bomb check: total unpacked size vs free disk space, plus a compression-ratio limit (configurable)
- [ ] Write to a temp folder next to the destination, move into place on success, delete it on cancel or error
- [ ] Return a report of skipped items (shown later as "2 items were skipped for your safety")

### Filename encoding
- [ ] Detect non-UTF-8 ZIP names; guess with `chardetng`; allow an override (CP1256, CP437, Shift-JIS, GBK, CP1251)

### Progress & cancel
- [ ] Progress callback: bytes done, files done, current path; cancel token checked between files and chunks

### CLI
- [ ] `crate l <archive>`, `crate x <archive> [-o dir] [--encoding cp1256]`, `crate t <archive>` (test integrity)

### Test fixtures (`tests/fixtures/`)
- [ ] Normal: zip, tar.zst, 7z, nested folders, unicode names
- [ ] Malicious: path traversal, absolute path, symlink escape, decompression bomb (small file, huge expansion)
- [ ] Old-encoding ZIP with Arabic CP1256 names

## Design references
- `EdgeCases.dc.html` (safety report, bomb warning) and `MiniViewRTL.dc.html` (encoding)

## Acceptance (on Ubuntu)
- [ ] `cargo test --workspace` passes, including every malicious fixture
- [ ] `crate x tests/fixtures/arabic-cp1256.zip --encoding cp1256` produces readable Arabic names
- [ ] Ctrl+C during a large extract leaves no partial files behind

## Prompt for Claude Code
> Read CLAUDE.md and docs/milestones/M1-core-extract.md. Build crate-core and crate-cli step by step: one format plus its tests per commit, starting with ZIP and the safety module. Generate the malicious fixtures with a small Rust test helper rather than downloading them.
