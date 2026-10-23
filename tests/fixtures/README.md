# Test archives

Sample and malicious archives used by `cargo test` (from M1 on). Keep each file small (ideally < 100 KB) and generate it with a script where possible, so the fixture can be rebuilt and reviewed.

## Planned layout

| Folder | Contents |
|---|---|
| `ok/` | One small, valid archive per format: `.zip` (stored + deflate), `.tar`, `.tar.gz`, `.tar.xz`, `.tar.zst`, `.tar.bz2`, `.7z`, `.rar` (RAR4 and RAR5, extract only) |
| `encrypted/` | ZIP (ZipCrypto + AES-256) and 7z with a known password, plus one with encrypted file names |
| `names/` | Old ZIPs with CP1256 (Arabic), CP437 and Shift-JIS file names; UTF-8 names with and without the UTF-8 flag; right-to-left names |
| `malicious/` | Must be **blocked** or contained by `crate_core::safety`: `../` path traversal ("zip-slip"), absolute paths (`/etc/…`, `C:\…`), symlinks pointing outside the destination, a symlink followed by a file written through it, hard links, decompression bombs (high ratio and nested), duplicate entries |
| `windows-names/` | Reserved names (`CON`, `AUX`, `NUL`, `COM1`…), illegal characters (`<>:"\|?*`), trailing dots/spaces, paths over 260 characters |
| `case/` | Entries whose names differ only by case (`Readme.md` + `README.md`) |
| `damaged/` | Truncated archives, bad CRCs, a ZIP with a broken central directory, a multi-part set with a missing volume |
| `nested/` | A zip inside a zip, a tar.gz inside a zip |

Each malicious fixture gets a test asserting that nothing is written outside the destination folder.
