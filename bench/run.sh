#!/usr/bin/env bash
# End-to-end latency gate with hyperfine.
#
# Scenarios (all with a frozen clock, a private cache dir and spawning
# disabled so only the tick itself is measured):
#   warm-default  seeded cache, default preset, inside a git repo
#   warm-full     seeded cache, full preset (the default rows, every option)
#   warm-all      seeded cache, one row holding every module id, so the
#                 settings-chain badges and the session-scoped `account`
#                 lookup are timed too
#   warm-tz       warm-default with TZ naming a zone (TZ=Europe/Berlin), the
#                 zone lookup containers and CI machines take
#   cold          empty cache each run, workers really spawned (first tick of a session)
#   refresh-sync  the background worker for one cached module
#
# Budgets (SPEC § 8): warm mean < 3 ms and p99 < 8 ms; cold mean < 30 ms;
# refresh mean < 50 ms. Results land in bench/results/*.json; check.sh gates.
set -euo pipefail
cd "$(dirname "$0")/.."

# The developer's shell must not reach the timed binary: GARNISH_DEBUG alone
# appends a debug.log line per tick.
unset GARNISH_DEBUG GARNISH_ANIMATE GARNISH_CONFIG TZ CLAUDE_CONFIG_DIR \
  CLAUDE_CODE_AUTO_COMPACT_WINDOW CLAUDE_AUTOCOMPACT_PCT_OVERRIDE \
  DISABLE_AUTO_COMPACT DISABLE_COMPACT

WARMUP="${WARMUP:-20}"
RUNS="${RUNS:-300}"
OUT=bench/results
mkdir -p "$OUT"

for tool in hyperfine jq git; do
  command -v "$tool" >/dev/null || { echo "bench: $tool is required (cargo install hyperfine)"; exit 1; }
done
cargo build --release --quiet
BIN="$PWD/target/release/garnish"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
repo="$work/repo"
origin="$work/origin.git"
# Cut off from the developer's git config: this project mandates signed
# commits, and the commit below cannot reach a pinentry from a script.
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
export GIT_AUTHOR_NAME=b GIT_AUTHOR_EMAIL=b@b GIT_COMMITTER_NAME=b GIT_COMMITTER_EMAIL=b@b
git init -q --bare -b main "$origin"
git init -q -b main "$repo"
(
  cd "$repo"
  git remote add origin "$origin"
  for i in $(seq 1 50); do echo "$i" > "f$i.txt"; done
  git add . && git commit -qm init
  git push -q -u origin main
  echo change > f1.txt
)

# A realistic payload whose cwd is the temp repo, in the session the seeding
# refresh below writes session-scoped entries for.
payload="$work/payload.json"
jq --arg cwd "$repo" '.cwd = $cwd | .workspace.current_dir = $cwd | .workspace.project_dir = $cwd
    | .session_id = "sess-bench"' \
  tests/fixtures/payloads/subscription-full.json > "$payload"

cache="$work/cache"
full_cfg="$work/full.toml"
printf 'preset = "full"\n' > "$full_cfg"
empty_cfg="$work/empty.toml"
: > "$empty_cfg"
# Every module id the binary knows, from `garnish modules` (the text.<name>
# family needs a definition, so it is left out).
all_cfg="$work/all.toml"
"$BIN" modules | awk '
  $1 !~ /^text\./ { ids = ids (ids == "" ? "" : ", ") "\"" $1 "\"" }
  END { print "[[row]]"; print "modules = [" ids "]" }
' > "$all_cfg"

export GARNISH_NOW=1738425600 GARNISH_NO_SPAWN=1 COLUMNS=120 HOME="$work"
export GARNISH_CACHE_DIR="$cache"
# No managed settings file: the gate measures garnish, not the machine's.
export GARNISH_MANAGED_SETTINGS=

# Seed the cache with a real refresh so warm ticks read entries within TTL.
"$BIN" --config "$empty_cfg" refresh --all --session sess-bench --cwd "$repo" >/dev/null

hyperfine --warmup "$WARMUP" --runs "$RUNS" -N --input "$payload" \
  --export-json "$OUT/warm-default.json" \
  -n warm-default "$BIN --config $empty_cfg"

hyperfine --warmup "$WARMUP" --runs "$RUNS" -N --input "$payload" \
  --export-json "$OUT/warm-full.json" \
  -n warm-full "$BIN --config $full_cfg"

hyperfine --warmup "$WARMUP" --runs "$RUNS" -N --input "$payload" \
  --export-json "$OUT/warm-all.json" \
  -n warm-all "$BIN --config $all_cfg"

# Exported for this run alone: `env TZ=… garnish` would time `env` too.
(
  export TZ=Europe/Berlin
  hyperfine --warmup "$WARMUP" --runs "$RUNS" -N --input "$payload" \
    --export-json "$OUT/warm-tz.json" \
    -n warm-tz "$BIN --config $empty_cfg"
)

# Cold: empty cache, and the tick really spawns its detached workers (that
# spawn is the dominant cold cost). The workers outlive the tick, so each
# prepare waits for the previous run's locks to go (a worker still writing
# would otherwise leave a fresh entry, and the next run would time a warm
# tick) before clearing the directory.
cold_cache="$work/cold-cache"
cold_prepare="$work/cold-prepare.sh"
cat > "$cold_prepare" <<EOF
#!/bin/sh
i=0
while [ -n "\$(find "$cold_cache" -name '*.lock' 2>/dev/null)" ] && [ \$i -lt 200 ]; do
  sleep 0.01
  i=\$((i + 1))
done
rm -rf "$cold_cache"
EOF
chmod +x "$cold_prepare"
hyperfine --warmup 5 --runs 100 --input "$payload" \
  --prepare "$cold_prepare" \
  --export-json "$OUT/cold.json" \
  -n cold "env -u GARNISH_NO_SPAWN GARNISH_CACHE_DIR=$cold_cache $BIN --config $empty_cfg"
sleep 1

hyperfine --warmup 5 --runs 50 -N \
  --export-json "$OUT/refresh-sync.json" \
  -n refresh-sync "$BIN --config $empty_cfg refresh --module sync --session sess-bench --cwd $repo"

./bench/check.sh
