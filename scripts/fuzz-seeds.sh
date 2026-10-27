#!/usr/bin/env bash
# Seed corpora for the fuzz targets (crates/crate-core/fuzz/corpus/<target>/):
# small valid archives made with the crate CLI, so the fuzzer starts from
# real structure. Run once; the files are committed.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q --release -p crate-cli
CRATE=$(realpath target/release/crate)
F=crates/crate-core/fuzz/corpus
src=$(mktemp -d)
trap 'rm -rf "$src"' EXIT
mkdir -p "$src/seed/docs"
echo "hello" > "$src/seed/a.txt"
printf 'x%.0s' {1..300} > "$src/seed/docs/b.md"
ln -s a.txt "$src/seed/link"
mkdir -p $F/zip $F/tar $F/tar_gz $F/sevenz
(cd "$src" && "$CRATE" c seed.zip seed >/dev/null && "$CRATE" c seed.tar seed >/dev/null \
  && "$CRATE" c seed.tar.gz seed >/dev/null && "$CRATE" c seed.7z seed >/dev/null)
cp "$src/seed.zip" $F/zip/
cp "$src/seed.tar" $F/tar/
cp "$src/seed.tar.gz" $F/tar_gz/
cp "$src/seed.7z" $F/sevenz/
ls -l $F/*
