#!/usr/bin/env bash
# End-to-end latency gate with hyperfine.
#
# Scenarios (all with a frozen clock, a private cache dir and spawning
# disabled so only the tick itself is measured):
#   warm-default  seeded cache, default preset, inside a git repo
#   warm-full     seeded cache, full preset (every module, every option)
#   cold          empty cache each run (first tick of a session)
#   refresh-sync  the background worker for one cached module
#
# Budgets (SPEC § 8): warm mean < 3 ms and p99 < 8 ms; cold mean < 30 ms;
# refresh mean < 50 ms. Results land in bench/results/*.json; check.sh gates.
set -euo pipefail
cd "$(dirname "$0")/.."

WARMUP="${WARMUP:-20}"
RUNS="${RUNS:-300}"
OUT=bench/results
mkdir -p "$OUT"

cargo build --release --quiet
BIN="$PWD/target/release/garnish"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
repo="$work/repo"
origin="$work/origin.git"
git init -q --bare -b main "$origin"
git clone -q "$origin" "$repo"
(
  cd "$repo"
  git checkout -q -b main
  for i in $(seq 1 50); do echo "$i" > "f$i.txt"; done
  git add . && git -c user.name=b -c user.email=b@b commit -qm init
  git push -q -u origin main
  echo change > f1.txt
)

# A realistic payload whose cwd is the temp repo.
payload="$work/payload.json"
jq --arg cwd "$repo" '.cwd = $cwd | .workspace.current_dir = $cwd | .workspace.project_dir = $cwd' \
  tests/fixtures/payloads/subscription-full.json > "$payload"

cache="$work/cache"
full_cfg="$work/full.toml"
printf 'preset = "full"\n' > "$full_cfg"
empty_cfg="$work/empty.toml"
: > "$empty_cfg"

export GARNISH_NOW=1738425600 GARNISH_NO_SPAWN=1 COLUMNS=120 HOME="$work"
export GARNISH_CACHE_DIR="$cache"

# Seed the cache with a real refresh so warm ticks read entries within TTL.
"$BIN" --config "$empty_cfg" refresh --all --session sess-bench --cwd "$repo" >/dev/null

hyperfine --warmup "$WARMUP" --runs "$RUNS" -N --input "$payload" \
  --export-json "$OUT/warm-default.json" \
  -n warm-default "$BIN --config $empty_cfg"

hyperfine --warmup "$WARMUP" --runs "$RUNS" -N --input "$payload" \
  --export-json "$OUT/warm-full.json" \
  -n warm-full "$BIN --config $full_cfg"

hyperfine --warmup 5 --runs 100 -N --input "$payload" \
  --prepare "rm -rf $cache" \
  --export-json "$OUT/cold.json" \
  -n cold "$BIN --config $empty_cfg"

hyperfine --warmup 5 --runs 50 -N \
  --export-json "$OUT/refresh-sync.json" \
  -n refresh-sync "$BIN --config $empty_cfg refresh --module sync --session sess-bench --cwd $repo"

./bench/check.sh
