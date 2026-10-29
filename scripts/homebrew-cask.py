#!/usr/bin/env python3
"""Writes the Homebrew cask for a published release into
target/homebrew/Casks/c/crate-archiver.rb, ready for a tap or a pull request
to Homebrew/homebrew-cask:

    scripts/homebrew-cask.py 0.3.0

The .dmg's SHA-256 comes from the release's SHA256SUMS on GitHub, so run it
after the release workflow has published that version (with the signed and
notarized .dmg: Homebrew won't take an app Gatekeeper blocks).
"""

import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = "https://github.com/mlbrainlab/crate"
# "crate" is taken in homebrew-cask; the Linux packages use this name too.
TOKEN = "crate-archiver"


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    version = sys.argv[1].removeprefix("v")
    dmg = f"Crate-{version}-universal.dmg"
    sums = urllib.request.urlopen(f"{REPO}/releases/download/v{version}/SHA256SUMS").read().decode()
    sha = next((line.split()[0] for line in sums.splitlines() if line.endswith(dmg)), None)
    if sha is None:
        print(f"{dmg} isn't in the release's SHA256SUMS", file=sys.stderr)
        return 1
    cask = f'''cask "{TOKEN}" do
  version "{version}"
  sha256 "{sha}"

  url "{REPO}/releases/download/v#{{version}}/Crate-#{{version}}-universal.dmg"
  name "Crate"
  desc "Open, extract and create archives"
  homepage "{REPO}"

  livecheck do
    url :url
    strategy :github_latest
  end

  depends_on macos: ">= :monterey"

  app "Crate.app"
  binary "#{{appdir}}/Crate.app/Contents/MacOS/crate"

  zap trash: [
    "~/.cache/crate",
    "~/.config/crate",
    "~/.local/state/crate",
  ]
end
'''
    out = ROOT / "target/homebrew/Casks/c"
    out.mkdir(parents=True, exist_ok=True)
    path = out / f"{TOKEN}.rb"
    path.write_text(cask)
    print(path.relative_to(ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
