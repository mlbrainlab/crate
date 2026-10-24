#!/usr/bin/env bash
# Startup benchmark (docs/perf.md): 10 warm + 10 cold launches of the mini view
# opening a real archive; the clock stops at the first frame that shows its
# listing. Cold runs drop the page cache before each launch, which needs sudo.
#
#   scripts/startup-bench.sh            # build, then warm + cold
#   scripts/startup-bench.sh --warm     # warm only (no sudo)
#   SKIP_BUILD=1 scripts/startup-bench.sh
#   ARCHIVE=~/Downloads/big.zip scripts/startup-bench.sh
#                                       # another archive (default: a generated
#                                       # 2,000-entry zip in $OUT)
#   BIN=path/to/other OUT=target/startup-bench-other scripts/startup-bench.sh
#                                       # another binary that prints the same crate-startup line
set -euo pipefail
cd "$(dirname "$0")/.."

RUNS=10
DEFAULT_BIN=target/release/crate-app
BIN=${BIN:-$DEFAULT_BIN}
OUT=${OUT:-target/startup-bench}
MODES=(warm cold)
[[ "${1:-}" == "--warm" ]] && MODES=(warm)

command -v hyperfine >/dev/null || { echo "hyperfine is missing: sudo apt install hyperfine" >&2; exit 1; }
if [[ -z "${SKIP_BUILD:-}" ]]; then
  if [[ $BIN == "$DEFAULT_BIN" ]]; then
    cargo build --release -p crate-app
  else
    echo "BIN=$BIN: build it yourself first."
  fi
fi
mkdir -p "$OUT"

# A plausible download: 2,000 files in nested folders, deflated, ~6 MB.
ARCHIVE=${ARCHIVE:-$OUT/bench.zip}
if [[ ! -f $ARCHIVE ]]; then
  python3 - "$ARCHIVE" <<'PY'
import random, sys, zipfile
rng = random.Random(42)
words = "alpha beta gamma delta photo report draft final notes data image page".split()
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED) as z:
    for i in range(2000):
        folder = f"project/{rng.choice(words)}/{rng.choice(words)}"
        body = " ".join(rng.choice(words) for _ in range(rng.randint(50, 900)))
        z.writestr(f"{folder}/{rng.choice(words)}-{i:04}.txt", body)
PY
fi
echo "Archive: $ARCHIVE"

if [[ " ${MODES[*]} " == *" cold "* ]]; then
  echo "Cold runs drop the page cache: sudo may ask for your password."
  sudo -v
fi

for mode in "${MODES[@]}"; do
  log="$OUT/$mode.log"
  : > "$log"
  args=(--runs "$RUNS" --export-json "$OUT/$mode.json" --command-name "$mode")
  if [[ $mode == warm ]]; then
    args+=(--warmup 3)
  else
    args+=(--prepare "sync; echo 3 | sudo tee /proc/sys/vm/drop_caches >/dev/null")
  fi
  # Warmup runs also log; the summary keeps only the last $RUNS lines.
  hyperfine "${args[@]}" "$BIN --startup-probe '$ARCHIVE' 2>>$log"
done

python3 - "$OUT" "$RUNS" "${MODES[@]}" <<'PY'
import json, re, statistics, sys
out, runs, modes = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
print(f"\n{'mode':<6} {'metric':<26} {'median':>8} {'min':>8} {'max':>8}  (ms)")
for mode in modes:
    lines = [l for l in open(f"{out}/{mode}.log") if l.startswith("crate-startup")][-runs:]
    vals = {k: [float(m.group(1)) for l in lines if (m := re.search(k + r"=([\d.]+)", l))]
            for k in ("premain_ms", "window_ms", "first_frame_ms", "ready_ms", "visible_ms")}
    total = [p + v for p, v in zip(vals["premain_ms"], vals["visible_ms"])]
    wall = [t * 1e3 for t in json.load(open(f"{out}/{mode}.json"))["results"][0]["times"]]
    rows = [("exec → main (pre-main)", vals["premain_ms"]),
            ("in-app: window built", vals["window_ms"]), ("in-app: first frame", vals["first_frame_ms"]),
            ("in-app: listing ready", vals["ready_ms"]),
            ("in-app: listing visible", vals["visible_ms"]),
            ("exec → visible (total)", total), ("hyperfine: exec → exit", wall)]
    for name, v in rows:
        if not v:
            continue
        print(f"{mode:<6} {name:<26} {statistics.median(v):>8.1f} {min(v):>8.1f} {max(v):>8.1f}  n={len(v)}")
PY
