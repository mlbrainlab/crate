#!/usr/bin/env bash
# Startup spike benchmark (docs/perf.md): 10 warm + 10 cold launches of the mini view.
# Cold runs drop the page cache before each launch, which needs sudo.
#
#   scripts/startup-bench.sh            # build, then warm + cold
#   scripts/startup-bench.sh --warm     # warm only (no sudo)
#   SKIP_BUILD=1 scripts/startup-bench.sh
set -euo pipefail
cd "$(dirname "$0")/.."

RUNS=10
BIN=target/release/crate-app
OUT=target/startup-bench
MODES=(warm cold)
[[ "${1:-}" == "--warm" ]] && MODES=(warm)

command -v hyperfine >/dev/null || { echo "hyperfine is missing: sudo apt install hyperfine" >&2; exit 1; }
if [[ -z "${SKIP_BUILD:-}" ]]; then
  pnpm tauri build --no-bundle
fi
mkdir -p "$OUT"

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
  hyperfine "${args[@]}" "$BIN --mini --startup-probe 2>>$log"
done

python3 - "$OUT" "$RUNS" "${MODES[@]}" <<'PY'
import json, re, statistics, sys
out, runs, modes = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
print(f"\n{'mode':<6} {'metric':<26} {'median':>8} {'min':>8} {'max':>8}  (ms)")
for mode in modes:
    lines = [l for l in open(f"{out}/{mode}.log") if l.startswith("crate-startup")][-runs:]
    vals = {k: [float(re.search(k + r"=([\d.]+)", l).group(1)) for l in lines]
            for k in ("window_ms", "ready_ms", "visible_ms")}
    wall = [t * 1e3 for t in json.load(open(f"{out}/{mode}.json"))["results"][0]["times"]]
    rows = [("in-app: window built", vals["window_ms"]), ("in-app: UI mounted", vals["ready_ms"]),
            ("in-app: visible frame", vals["visible_ms"]), ("hyperfine: exec → exit", wall)]
    for name, v in rows:
        print(f"{mode:<6} {name:<26} {statistics.median(v):>8.1f} {min(v):>8.1f} {max(v):>8.1f}  n={len(v)}")
PY
