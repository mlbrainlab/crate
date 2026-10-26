#!/usr/bin/env bash
# Collects every translatable string into app/translations/crate-app.pot and
# merges it into each language's .po (new strings arrive untranslated, changed
# ones are marked fuzzy). Slint: @tr() in app/ui/*.slint (slint-tr-extractor,
# `cargo install slint-tr-extractor --version 1.18.1`). Rust: tr("…"),
# trf("…", …) and trn("…", "…", n) in app/src (see app/src/i18n.rs).
set -euo pipefail
cd "$(dirname "$0")/../app"
POT=translations/crate-app.pot

slint-tr-extractor --no-default-translation-context --package-name Crate \
  -o "$POT" ui/*.slint
python3 - "$POT" src <<'PY'
import pathlib, re, sys
pot, src = sys.argv[1], pathlib.Path(sys.argv[2])
known = set(re.findall(r'^msgid "((?:[^"\\]|\\.)*)"', open(pot, encoding="utf-8").read(), re.M))
lit = r'"((?:[^"\\]|\\.)*)"'
plural = re.compile(r'\btrn\(\s*' + lit + r'\s*,\s*' + lit, re.S)
single = re.compile(r'\btr[f]?\(\s*' + lit, re.S)
out = []
for f in sorted(src.rglob("*.rs")):
    text = f.read_text(encoding="utf-8")
    rel = f.as_posix()
    for m in plural.finditer(text):
        if m.group(1) in known: continue
        known.add(m.group(1))
        line = text.count("\n", 0, m.start()) + 1
        out.append(f'#: {rel}:{line}\nmsgid "{m.group(1)}"\nmsgid_plural "{m.group(2)}"\nmsgstr[0] ""\nmsgstr[1] ""\n')
    for m in single.finditer(text):
        if m.group(1) in known: continue
        known.add(m.group(1))
        line = text.count("\n", 0, m.start()) + 1
        out.append(f'#: {rel}:{line}\nmsgid "{m.group(1)}"\nmsgstr ""\n')
with open(pot, "a", encoding="utf-8") as fh:
    for o in out: fh.write("\n" + o)
PY
for po in translations/*/LC_MESSAGES/crate-app.po; do
  msgmerge --quiet --update --backup=none --no-fuzzy-matching --no-wrap "$po" "$POT"
  msgfmt --check --statistics -o /dev/null "$po"
done
