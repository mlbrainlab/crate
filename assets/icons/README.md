# Crate app icon: concept #1 "Open Lid"

| File | Use |
|---|---|
| `crate-icon.svg` | Master artwork, full-bleed rounded tile (Linux, Windows, README, website) |
| `crate-icon-macos.svg` | macOS master: an 824 px tile with a shadow on a 1024 canvas, the size macOS expects |
| `png/crate-{16…1024}.png` | Rendered sizes for Linux (hicolor theme, Flatpak/AppStream) and docs |
| `png/crate-macos-1024.png` | Source for the macOS icon |
| `crate.ico` | Windows (16–256) |
| `crate.icns` | macOS |

## With Tauri (M0)
```
pnpm tauri icon assets/icons/png/crate-1024.png
```
This generates every platform file into `app/src-tauri/icons/`. Afterwards, replace `icon.icns` there with `assets/icons/crate.icns`, so macOS gets the padded version.

Colors: tile `#3352E0`, lid `#DDE4FF`, box `#FFFFFF`, document `#FFD27A`.
