#!/usr/bin/env bash
# Gate bench/results/*.json against the latency budgets. Exit 1 when over.
set -euo pipefail
cd "$(dirname "$0")/.."

# name  mean_ms_max  p99_ms_max (0 = not gated)
BUDGETS="warm-default 3 8
warm-full 3 8
cold 30 0
refresh-sync 50 0"

fail=0
printf '%-14s %10s %10s %10s %10s  %s\n' scenario mean p99 max budget status
while read -r name mean_max p99_max; do
  f="bench/results/$name.json"
  [[ -f "$f" ]] || { echo "missing $f"; fail=1; continue; }
  line="$(jq -r --argjson mm "$mean_max" --argjson pm "$p99_max" '
      .results[0] | (.times | sort) as $t
      | (.mean * 1000) as $mean
      | ($t[((($t | length) * 0.99 | ceil) - 1)] * 1000) as $p99
      | (.max * 1000) as $max
      | (if $mean > $mm or ($pm > 0 and $p99 > $pm) then "OVER" else "ok" end) as $status
      | [($mean * 1000 | round / 1000), ($p99 * 1000 | round / 1000), ($max * 1000 | round / 1000), $status]
      | @tsv' "$f")"
  IFS=$'\t' read -r mean p99 max status <<< "$line"
  [[ $status == ok ]] || fail=1
  printf '%-14s %8sms %8sms %8sms %10s  %s\n' "$name" "$mean" "$p99" "$max" "<$mean_max/$p99_max" "$status"
done <<< "$BUDGETS"
exit $fail
