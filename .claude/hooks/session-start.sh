#!/usr/bin/env bash
# Claude Code SessionStart hook (wired in .claude/settings.json).
#
# Detects the host with scripts/session-host.sh, exports SESSION_HOST for
# every Bash command in the session (through CLAUDE_ENV_FILE), and prints
# the host-specific context Claude should see. Only a Sprite gets SPRITE.md
# loaded; on a personal machine the host facts live in the user-level
# ~/.claude/CLAUDE.md, which Claude Code loads on its own, so this hook
# prints just a pointer. Runs again on resume, clear and compact, which is
# what keeps the context fresh.
set -u
root="${CLAUDE_PROJECT_DIR:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$root" || exit 0

host="$(scripts/session-host.sh)"

if [ -n "${CLAUDE_ENV_FILE:-}" ]; then
  printf 'export SESSION_HOST=%s\n' "$host" >> "$CLAUDE_ENV_FILE"
fi

echo "SESSION_HOST=$host (set by .claude/hooks/session-start.sh; scripts/setup.sh installs the prerequisites for this host)."
case "$host" in
  sprite)
    echo "This session runs on a Sprite. SPRITE.md, the Sprite working notes, follows."
    echo
    cat SPRITE.md
    ;;
  popos | macos)
    echo "This session runs on a personal machine. Host facts (commit signing, GitHub auth, how tools are installed) come from the user-level ~/.claude/CLAUDE.md, not from this repository."
    ;;
  ci)
    ;;
  *)
    echo "This host is not recognised; no host-specific notes are loaded."
    ;;
esac
exit 0
