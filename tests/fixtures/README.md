# Test archives

Sample and malicious archives. The tests don't read this folder: they build their own copies with the generators in `crates/crate-core/tests/support/`. The files here are the same archives, for trying the CLI by hand:

```
cargo run -p crate-core --example gen_fixtures    # regenerate everything
cargo run -p crate-cli -- x tests/fixtures/arabic-cp1256.zip --encoding cp1256
```

Keep each file small (ideally < 100 KB) and add new ones to the generator rather than committing archives from elsewhere.

## RAR samples (`rar/`)
RAR can't be generated here: there is no free RAR writer, and UnRAR's license forbids building one. `rar/*.rar` are copied from the test data of the [`unrar`](https://github.com/muja/unrar.rs) crate (MIT / Apache-2.0): `solid.rar`, `comment.rar`, `unicode.rar`, `crypted.rar`. The RAR tests also make damaged copies on the fly.

## Generated so far

| File | What it checks |
|---|---|
| `sample.zip` | Nested folders, a Unicode name, a setuid bit that must be dropped |
| `arabic-cp1256.zip` | Arabic names stored as Windows-1256 without the UTF-8 flag |
| `encrypted/aes256.zip` | One AES-256 entry (password `secret`) next to a plain one |
| `damaged/bad-crc.zip` | An entry whose CRC doesn't match its data |
| `malicious/traversal.zip` | `../`, `..\`, absolute and drive-letter names |
| `malicious/symlinks.zip` | Links pointing outside, and files written through links |
| `malicious/lying-size.zip` | An entry with more data than its header declares |
| `malicious/bomb.zip` | 100 KB that expands to 100 MiB (about 1000×) |
| `sample.tar.gz`, `sample.tar.zst` | The sample as tar, plus a hard link |
| `malicious/escape.tar.gz` | `../` and absolute names, an escaping symlink and hard links, a device node |
| `malicious/bomb.tar.zst` | A few hundred bytes that expand to 512 MiB |
| `damaged/truncated.tar.gz` | A tar.gz cut off halfway |
| `sample.7z` | The sample as a solid 7z, with a symlink |
| `malicious/escape.7z` | `../` and absolute names, an escaping symlink |
| `encrypted/aes256.7z`, `encrypted/aes256-names.7z` | AES-256 7z (password `secret`); the second also hides the file names |

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
