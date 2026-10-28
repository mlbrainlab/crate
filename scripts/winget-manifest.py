#!/usr/bin/env python3
"""Writes the winget manifests for a published release into
target/winget/manifests/m/mlbrainlab/Crate/<version>/, ready to copy into a
fork of microsoft/winget-pkgs (docs/winget.md):

    scripts/winget-manifest.py 0.2.0

The installer's SHA-256 comes from the release's SHA256SUMS on GitHub, so
run it after the release workflow has published that version.
"""

import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = "https://github.com/mlbrainlab/crate"
ID = "mlbrainlab.Crate"
SCHEMA = "1.10.0"


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    version = sys.argv[1].removeprefix("v")
    setup = f"Crate-{version}-x64-setup.exe"
    url = f"{REPO}/releases/download/v{version}/{setup}"
    sums = urllib.request.urlopen(f"{REPO}/releases/download/v{version}/SHA256SUMS").read().decode()
    sha = next((line.split()[0] for line in sums.splitlines() if line.endswith(setup)), None)
    if sha is None:
        print(f"{setup} isn't in the release's SHA256SUMS", file=sys.stderr)
        return 1
    header = lambda kind: (
        f"# yaml-language-server: $schema=https://aka.ms/winget-manifest.{kind}.{SCHEMA}.schema.json\n"
    )
    files = {
        f"{ID}.yaml": header("version")
        + f"""PackageIdentifier: {ID}
PackageVersion: {version}
DefaultLocale: en-US
ManifestType: version
ManifestVersion: {SCHEMA}
""",
        f"{ID}.installer.yaml": header("installer")
        + f"""PackageIdentifier: {ID}
PackageVersion: {version}
InstallerType: inno
MinimumOSVersion: 10.0.0.0
UpgradeBehavior: install
FileExtensions: [zip, 7z, rar, tar, tgz, gz, txz, xz, tzst, zst, tbz2, bz2, "001"]
Commands: [crate]
Installers:
  - Architecture: x64
    Scope: user
    InstallerUrl: {url}
    InstallerSha256: {sha.upper()}
    InstallerSwitches:
      Custom: /CURRENTUSER
  - Architecture: x64
    Scope: machine
    InstallerUrl: {url}
    InstallerSha256: {sha.upper()}
    ElevationRequirement: elevatesSelf
    InstallerSwitches:
      Custom: /ALLUSERS
ManifestType: installer
ManifestVersion: {SCHEMA}
""",
        f"{ID}.locale.en-US.yaml": header("defaultLocale")
        + f"""PackageIdentifier: {ID}
PackageVersion: {version}
PackageLocale: en-US
Publisher: mlbrainlab
PublisherUrl: https://github.com/mlbrainlab
PublisherSupportUrl: {REPO}/issues
PackageName: Crate
PackageUrl: {REPO}
License: MIT (RAR extraction uses the freeware UnRAR library)
LicenseUrl: {REPO}/blob/v{version}/LICENSE
ShortDescription: Open, extract and create archives
Description: |-
  Crate opens ZIP, 7z, RAR and tar archives (gz, xz, zst, bz2) and makes ZIP, 7z and tar ones.
  A small window shows what is inside, one click from extracting everything; the full view browses, previews and edits archives.
  Unsafe paths, links and decompression bombs are stopped. Includes the `crate` command-line tool.
Moniker: crate
Tags: [archive, archiver, zip, unzip, 7z, rar, tar, extract, compress]
ReleaseNotesUrl: {REPO}/releases/tag/v{version}
ManifestType: defaultLocale
ManifestVersion: {SCHEMA}
""",
    }
    out = ROOT / "target/winget/manifests/m/mlbrainlab/Crate" / version
    out.mkdir(parents=True, exist_ok=True)
    for name, text in files.items():
        (out / name).write_text(text)
    print(out.relative_to(ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
