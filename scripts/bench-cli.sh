#!/usr/bin/env bash
# The README's benchmarks: the `crate` CLI against the usual tools, with
# hyperfine, on data this script generates in target/bench/ (about 2 GB of
# disk). Warm cache; every run starts from a clean output folder.
#
#   scripts/bench-cli.sh            # build release, generate data, run
#   SKIP_BUILD=1 scripts/bench-cli.sh
set -euo pipefail
cd "$(dirname "$0")/.."

for tool in hyperfine unzip zip tar zstd python3; do
  command -v $tool >/dev/null || { echo "needs $tool" >&2; exit 1; }
done
[[ -n "${SKIP_BUILD:-}" ]] || cargo build --release -p crate-cli
CRATE=$(realpath target/release/crate)
B=target/bench
mkdir -p "$B"

# 1 GB in 2,000 files: half text (compresses well), half random (doesn't).
if [[ ! -d $B/data ]]; then
  python3 - "$B/data" <<'PY'
import os, random, sys
root = sys.argv[1]
rng = random.Random(42)
words = "archive folder extract crate safety zstd deflate entry header volume tar zip".split()
for i in range(2000):
    d = os.path.join(root, f"dir{i % 40:02d}")
    os.makedirs(d, exist_ok=True)
    if i % 2:
        data = rng.randbytes(500_000)
    else:
        data = " ".join(rng.choice(words) for _ in range(90_000)).encode()[:500_000]
    with open(os.path.join(d, f"file{i:04d}.bin" if i % 2 else f"file{i:04d}.txt"), "wb") as f:
        f.write(data)
PY
fi
# 120,000 small files for the listing test.
if [[ ! -f $B/many.tar.gz ]]; then
  python3 - "$B/many.tar.gz" <<'PY'
import io, tarfile, sys
with tarfile.open(sys.argv[1], "w:gz") as t:
    for i in range(120_000):
        data = f"entry {i}\n".encode()
        info = tarfile.TarInfo(f"many/d{i // 1000:03d}/f{i:06d}.txt")
        info.size = len(data)
        t.addfile(info, io.BytesIO(data))
PY
fi
[[ -f $B/data.zip ]] || (cd "$B" && zip -qr -6 data.zip data)

echo "== Extract a 1 GB ZIP"
hyperfine --warmup 1 --runs 5 --export-markdown "$B/extract.md" \
  --prepare "rm -rf $B/out" \
  -n "crate x" "$CRATE x $B/data.zip -o $B/out" \
  -n "unzip" "unzip -q $B/data.zip -d $B/out"

# Crate flushes a new archive to disk before reporting success (so "Move
# originals to Trash" never trashes anything the disk doesn't have yet);
# tar's file is synced too, to compare like with like.
echo "== Create a tar.zst of 1 GB (zstd level 3, synced to disk)"
hyperfine --warmup 1 --runs 5 --export-markdown "$B/create.md" \
  --prepare "rm -f $B/new.tar.zst; sync" \
  -n "crate c" "$CRATE c $B/new.tar.zst $B/data --level 3" \
  -n "tar --zstd" "tar -C $B -I 'zstd -3 -T0' -cf $B/new.tar.zst data && sync $B/new.tar.zst"

echo "== List a tar.gz with 120,000 entries"
hyperfine --warmup 1 --runs 5 --export-markdown "$B/list.md" \
  -n "crate l" "$CRATE l $B/many.tar.gz > /dev/null" \
  -n "tar -t" "tar -tzf $B/many.tar.gz > /dev/null"

echo
cat "$B/extract.md" "$B/create.md" "$B/list.md"
