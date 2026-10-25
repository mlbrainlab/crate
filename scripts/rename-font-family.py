#!/usr/bin/env python3
"""Renames the bundled Inter faces to the family "Crate Inter".

Why: a system-wide Inter (often the variable font) shares the family name,
and Slint's GPU renderer then picks the system one over the bundled faces
and draws it without its variation settings (narrow, wrong weight). A
family name of our own can't collide. Inter's OFL declares no Reserved
Font Name, so a renamed copy is allowed; it stays under the OFL.

Needs fontTools:  pip install --target /tmp/ft fonttools
Run:              PYTHONPATH=/tmp/ft scripts/rename-font-family.py
"""
from pathlib import Path

from fontTools.ttLib import TTFont

FAMILY = "Crate Inter"
FONTS = Path(__file__).resolve().parent.parent / "assets" / "fonts" / "inter"

for path in sorted(FONTS.glob("Inter-*.ttf")):
    font = TTFont(path)
    name = font["name"]
    style = path.stem.split("-", 1)[1]  # Regular, Medium, SemiBold, Bold
    # Weights other than Regular/Bold are their own legacy family (as Inter
    # ships them); the typographic family (16/17) ties the four together.
    legacy_family = FAMILY if style in ("Regular", "Bold") else f"{FAMILY} {style}"
    legacy_style = style if style in ("Regular", "Bold") else "Regular"
    values = {
        1: legacy_family,
        2: legacy_style,
        3: f"{FAMILY.replace(' ', '')}-{style};crate",
        4: f"{FAMILY} {style}",
        6: f"{FAMILY.replace(' ', '')}-{style}",
        16: FAMILY,
        17: style,
    }
    for record in list(name.names):
        if record.nameID in (16, 17, 21, 22, 25):
            name.removeNames(nameID=record.nameID)
    for name_id, value in values.items():
        name.setName(value, name_id, 3, 1, 0x409)  # Windows, Unicode, English
        name.setName(value, name_id, 1, 0, 0)  # Mac Roman, English
    font.save(path)
    print(f"{path.name}: {FAMILY} {style}")
