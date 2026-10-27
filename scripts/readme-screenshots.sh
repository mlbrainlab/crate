#!/usr/bin/env bash
# Renders the README's and AppStream's screenshots (app/tests/readme_shots.rs)
# and copies them to docs/screenshots/.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test -p crate-app --test readme_shots
mkdir -p docs/screenshots
cp target/ui-snapshots/readme/{mini,full,dark,create,arabic}.png docs/screenshots/
ls -l docs/screenshots
