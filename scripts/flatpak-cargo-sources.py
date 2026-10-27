#!/usr/bin/env python3
"""Writes packaging/flatpak/cargo-sources.json from Cargo.lock: every crate
as a flatpak-builder archive source (the .crate file from static.crates.io,
checked against Cargo.lock's sha256), unpacked into cargo/vendor/, plus the
cargo config that builds from there. Flathub builds offline, so the manifest
needs this. Run it again whenever Cargo.lock changes:

    scripts/flatpak-cargo-sources.py

Same output format as flatpak-builder-tools' flatpak-cargo-generator, for
crates.io dependencies only (Crate has no git dependencies).
"""

import json
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LOCK = ROOT / "Cargo.lock"
OUT = ROOT / "packaging/flatpak/cargo-sources.json"
CRATES_IO = "registry+https://github.com/rust-lang/crates.io-index"
VENDOR = "cargo/vendor"


def main() -> int:
    lock = tomllib.loads(LOCK.read_text())
    sources = []
    for pkg in lock["package"]:
        source = pkg.get("source")
        if source is None:
            continue  # the workspace's own crates
        if source != CRATES_IO:
            print(f"unsupported source for {pkg['name']}: {source}", file=sys.stderr)
            return 1
        name, version, checksum = pkg["name"], pkg["version"], pkg["checksum"]
        dest = f"{VENDOR}/{name}-{version}"
        sources.append(
            {
                "type": "archive",
                "archive-type": "tar-gzip",
                "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
                "sha256": checksum,
                "dest": dest,
            }
        )
        # cargo checks vendored crates against this file; an empty file list
        # means "trust the package checksum".
        sources.append(
            {
                "type": "inline",
                "contents": json.dumps({"package": checksum, "files": {}}),
                "dest": dest,
                "dest-filename": ".cargo-checksum.json",
            }
        )
    sources.append(
        {
            "type": "inline",
            "contents": (
                "[source.vendored-sources]\n"
                f'directory = "{VENDOR}"\n\n'
                "[source.crates-io]\n"
                'replace-with = "vendored-sources"\n'
            ),
            "dest": "cargo",
            "dest-filename": "config",
        }
    )
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(sources, indent=4) + "\n")
    print(f"{OUT.relative_to(ROOT)}: {len(sources) // 2} crates")
    return 0


if __name__ == "__main__":
    sys.exit(main())
