# Crate Arabic

IBM Plex Sans Arabic 1.1.0 (Medium and SemiBold), renamed to the family "Crate Arabic" by `scripts/rename-font-family.py`.

- Source: https://github.com/IBM/plex/releases/tag/%40ibm/plex-sans-arabic%401.1.0, `ibm-plex-sans-arabic.zip` (sha256 `f03915581aea37d82792c188b08064023a73494d679b8e19f85f5971db714013`), `fonts/complete/ttf/`.
- License: SIL Open Font License 1.1 (`LICENSE.txt`). "Plex" is a Reserved Font Name, so the modified copies here carry another name. Nothing else is changed: same glyphs, same metrics.
- It has its own Latin set, so the Arabic UI can use it for all text. Slint doesn't use bundled fonts for fallback, so "Crate Arabic" applies only where a `font-family` names it (M5 notes).
