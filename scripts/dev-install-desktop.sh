#!/usr/bin/env bash
# Installs Crate's .desktop file and icons for the current user, so Ubuntu
# Files offers "Open With → Crate" for zip, 7z, rar and tar archives. The
# entry runs this checkout's release build. Crate does not become the default
# app for anything; pick it in Open With, or set it as default there yourself.
#
#   scripts/dev-install-desktop.sh              # build release, install
#   SKIP_BUILD=1 scripts/dev-install-desktop.sh
#   scripts/dev-install-desktop.sh --uninstall
set -euo pipefail
cd "$(dirname "$0")/.."

APP_ID=io.github.mlbrainlab.crate
DATA=${XDG_DATA_HOME:-$HOME/.local/share}
APPS=$DATA/applications
ICONS=$DATA/icons/hicolor
SIZES=(16 24 32 48 64 128 256 512)

refresh() {
  update-desktop-database -q "$APPS" 2>/dev/null || true
  # Refresh an existing user icon cache; never create one (a stale cache
  # would hide icons other apps install later).
  if [[ -f $ICONS/icon-theme.cache ]]; then
    gtk-update-icon-cache -q -t "$ICONS" 2>/dev/null || true
  fi
}

if [[ "${1:-}" == "--uninstall" ]]; then
  rm -f "$APPS/$APP_ID.desktop"
  for s in "${SIZES[@]}"; do rm -f "$ICONS/${s}x${s}/apps/$APP_ID.png"; done
  refresh
  echo "Removed $APPS/$APP_ID.desktop and the icons."
  exit 0
fi

if [[ -z "${SKIP_BUILD:-}" ]]; then
  cargo build --release -p crate-app
fi
BIN=$(realpath target/release/crate-app)
[[ -x $BIN ]] || { echo "$BIN is missing: build it first" >&2; exit 1; }

mkdir -p "$APPS"
# Quote the path for the Exec key in case the checkout path has spaces.
sed "s|@EXEC@|\"$BIN\"|g" packaging/linux/$APP_ID.desktop > "$APPS/$APP_ID.desktop"
if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$APPS/$APP_ID.desktop"
fi
for s in "${SIZES[@]}"; do
  install -Dm644 "assets/icons/png/crate-$s.png" "$ICONS/${s}x${s}/apps/$APP_ID.png"
done
refresh

echo "Installed $APPS/$APP_ID.desktop -> $BIN"
echo "In Files: right-click an archive → Open With… → Crate."
