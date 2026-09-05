#!/usr/bin/env bash
# Install or update everything the Makefile targets need, on any host.
#
#   scripts/setup.sh            rustup, the nightly toolchain from
#                               rust-toolchain.toml (updated to the latest
#                               nightly, with rustfmt/clippy/rust-analyzer/
#                               rust-src), and cargo-nextest
#   scripts/setup.sh --bench    + hyperfine and jq (make bench)
#   scripts/setup.sh --all      + watchexec (make watch)
#
# The host class comes from SESSION_HOST (scripts/session-host.sh) and
# decides *how* a tool is installed, never *whether*:
#
#   ci      prebuilt cargo-nextest from get.nexte.st; hyperfine/jq from the
#           runner's package manager (apt on Linux, brew on macOS)
#   popos   cargo install --locked (devup's cargobins section then keeps
#           every cargo-installed crate current); rustup itself is expected
#           to be present already (devup's rust section owns it)
#   macos   brew for hyperfine/jq when brew exists, cargo install otherwise
#   sprite  cargo install --locked; rustup is installed if missing
#   unknown same as sprite
#
# Idempotent: re-running only updates what is out of date.
set -euo pipefail
cd "$(dirname "$0")/.."

host="${SESSION_HOST:-$(scripts/session-host.sh)}"
want_bench=0
want_all=0
for arg in "$@"; do
  case "$arg" in
    --bench) want_bench=1 ;;
    --all) want_bench=1; want_all=1 ;;
    -h | --help) sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "setup: unknown flag $arg" >&2; exit 2 ;;
  esac
done

log() { printf '\n\033[1;36m== %s\033[0m\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }
os() { case "$(uname -s)" in Darwin) echo mac ;; *) echo linux ;; esac; }
sudo_cmd=""
if [ "$(id -u)" != 0 ] && have sudo; then sudo_cmd="sudo"; fi

# Installs a package through the host's package manager (apt or brew).
pkg_install() {
  if have apt-get; then
    $sudo_cmd apt-get update -qq
    $sudo_cmd apt-get install -y --no-install-recommends "$@"
  elif have brew; then
    brew install "$@"
  else
    return 1
  fi
}

log "host: $host"

# --- rustup ---------------------------------------------------------------
if ! have rustup; then
  case "$host" in
    popos)
      echo "setup: rustup is missing; install it with devup (rust section) or https://rustup.rs and re-run." >&2
      exit 1
      ;;
    *)
      log "installing rustup"
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --default-toolchain none
      export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
      ;;
  esac
fi

# --- toolchain: rust-toolchain.toml, then the latest nightly ---------------
channel="$(sed -n 's/^channel *= *"\([^"]*\)".*/\1/p' rust-toolchain.toml)"
log "rustup toolchain install $channel (components from rust-toolchain.toml)"
# No-argument form (rustup >= 1.28) reads rust-toolchain.toml. Plain if/else
# rather than an optional array: an empty array trips `set -u` on bash 3.2.
if [ "$host" = ci ]; then
  rustup toolchain install --profile minimal
else
  rustup toolchain install
fi
log "rustup update $channel"
rustup update --no-self-update "$channel"
rustc --version

# --- cargo-nextest ---------------------------------------------------------
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
if have cargo-nextest; then
  log "cargo-nextest present: $(cargo nextest --version | head -n 1)"
elif [ "$host" = ci ]; then
  log "cargo-nextest (prebuilt from get.nexte.st)"
  mkdir -p "$cargo_bin"
  curl -LsSf "https://get.nexte.st/latest/$(os)" | tar zxf - -C "$cargo_bin"
else
  log "cargo install cargo-nextest"
  cargo install --locked cargo-nextest
fi

# --- bench tools: hyperfine, jq --------------------------------------------
if [ "$want_bench" = 1 ]; then
  if have hyperfine; then
    log "hyperfine present: $(hyperfine --version)"
  else
    case "$host" in
      ci) log "hyperfine (package manager)"; pkg_install hyperfine ;;
      macos) log "hyperfine"; if have brew; then brew install hyperfine; else cargo install --locked hyperfine; fi ;;
      *) log "cargo install hyperfine"; cargo install --locked hyperfine ;;
    esac
  fi
  if have jq; then
    log "jq present: $(jq --version)"
  else
    log "jq (package manager)"
    pkg_install jq || { echo "setup: install jq with your package manager and re-run." >&2; exit 1; }
  fi
fi

# --- watchexec (make watch) ------------------------------------------------
if [ "$want_all" = 1 ] && [ "$host" != ci ]; then
  if have watchexec; then
    log "watchexec present: $(watchexec --version | head -n 1)"
  else
    log "cargo install watchexec-cli"
    cargo install --locked watchexec-cli
  fi
fi

log "ready"
printf '  %-14s %s\n' rustc "$(rustc --version)" cargo "$(cargo --version)" nextest "$(cargo nextest --version | head -n 1)"
[ "$want_bench" = 1 ] && printf '  %-14s %s\n' hyperfine "$(hyperfine --version)" jq "$(jq --version)"
exit 0
