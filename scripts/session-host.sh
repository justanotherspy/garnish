#!/usr/bin/env bash
# Print the name of the host class this shell runs on, one word on stdout:
#
#   sprite   a Sprite VM (/.sprite exists or sprite-env is on PATH)
#   ci       a GitHub Actions runner (or any host with CI set)
#   macos    Darwin
#   popos    Pop!_OS (ID=pop in /etc/os-release)
#   unknown  anything else
#
# An already-set SESSION_HOST wins, so a host can be forced for testing
# (SESSION_HOST=sprite scripts/setup.sh). Used by the SessionStart hook in
# .claude/hooks/session-start.sh and by scripts/setup.sh; keep the detection
# here so both agree.
set -u

if [ -n "${SESSION_HOST:-}" ]; then
  printf '%s\n' "$SESSION_HOST"
  exit 0
fi
if [ -d /.sprite ] || command -v sprite-env >/dev/null 2>&1; then
  echo sprite
elif [ "${GITHUB_ACTIONS:-}" = "true" ] || [ -n "${CI:-}" ]; then
  echo ci
elif [ "$(uname -s)" = "Darwin" ]; then
  echo macos
elif [ -r /etc/os-release ] && grep -q '^ID=pop$' /etc/os-release; then
  echo popos
else
  echo unknown
fi
