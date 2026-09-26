#!/usr/bin/env bash
# Install or update everything the Makefile targets need, on any host.
#
#   scripts/setup.sh            rustup, the nightly toolchain from
#                               rust-toolchain.toml (updated to the latest
#                               nightly, with rustfmt/clippy/rust-analyzer/
#                               rust-src), cargo-nextest and shellcheck
#   scripts/setup.sh --bench    + hyperfine and jq (make bench)
#   scripts/setup.sh --all      + watchexec (make watch)
#
# The host class comes from SESSION_HOST (scripts/session-host.sh) and
# decides *how* a tool is installed, never *whether*:
#
#   ci      pinned, checksummed cargo-nextest from get.nexte.st; shellcheck, hyperfine
#           and jq from the runner's package manager (apt on Linux, brew on
#           macOS)
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
    -h | --help) awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "$0"; exit 0 ;;
    *) echo "setup: unknown flag $arg" >&2; exit 2 ;;
  esac
done

log() { printf '\n\033[1;36m== %s\033[0m\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }
# The get.nexte.st platform name: mac (universal), linux, linux-arm.
os() {
  case "$(uname -s)/$(uname -m)" in
    Darwin/*) echo mac ;;
    */aarch64 | */arm64) echo linux-arm ;;
    *) echo linux ;;
  esac
}
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

# --- proxies: cargo and rustc must be on PATH --------------------------------
# rustup's proxies are not always next to rustup itself: rustup-init puts them
# in $CARGO_HOME/bin, a package-manager rustup keeps them in its own bin
# directory (Homebrew's formula is keg-only, so only `rustup` is linked).
# Without them every cargo command fails with "command not found", so find
# them, use them for the rest of this run, and refuse to finish until the
# shell's PATH has them too.
proxy_dir=""
if ! have cargo || ! have rustc; then
  candidates=("${CARGO_HOME:-$HOME/.cargo}/bin")
  if have brew; then candidates+=("$(brew --prefix rustup 2>/dev/null || true)/bin"); fi
  for dir in "${candidates[@]}"; do
    if [ -x "$dir/cargo" ] && [ -x "$dir/rustc" ]; then proxy_dir="$dir"; break; fi
  done
  if [ -z "$proxy_dir" ]; then
    echo "setup: rustup installed $channel but its cargo and rustc proxies are in none of: ${candidates[*]}" >&2
    echo "setup: see https://rust-lang.github.io/rustup/installation/already-installed-rust.html" >&2
    exit 1
  fi
  export PATH="$proxy_dir:$PATH"
fi
rustc --version

# --- cargo-nextest ---------------------------------------------------------
# CI takes a pinned prebuilt binary and checks it before running it: a
# download of "latest" executed unverified is a supply-chain door into every
# job that runs this script. The release has no published checksums, so these
# are the sha256 of the release assets as downloaded when the pin was set;
# bump the version and all three together.
nextest_version=0.9.146
nextest_sha256() {
  case "$1" in
    linux) echo 682c21b777c333e96fd532e114d3a5a894e0729ab88d94c0a9f20f8419695428 ;;
    linux-arm) echo b2e33d7c72de7ade0ff7b3a948ac37516b24f8a836b7a8870c1f634a94be9de9 ;;
    mac) echo 39785160b3c2f6ed9a765049cf4fa79f3b39aa02eb7598a5a0e2a1a0b9ffb9a8 ;;
  esac
}
sha256_of() {
  if have sha256sum; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi
}
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
if have cargo-nextest; then
  log "cargo-nextest present: $(cargo nextest --version | head -n 1)"
elif [ "$host" = ci ]; then
  log "cargo-nextest $nextest_version (prebuilt from get.nexte.st, checksummed)"
  mkdir -p "$cargo_bin"
  archive="$(mktemp)"
  curl -LsSf -o "$archive" "https://get.nexte.st/$nextest_version/$(os)"
  want="$(nextest_sha256 "$(os)")"
  got="$(sha256_of "$archive")"
  if [ "$got" != "$want" ]; then
    echo "setup: cargo-nextest $nextest_version for $(os) has sha256 $got, expected $want; refusing to install it" >&2
    rm -f "$archive"
    exit 1
  fi
  tar zxf "$archive" -C "$cargo_bin"
  rm -f "$archive"
else
  log "cargo install cargo-nextest"
  cargo install --locked cargo-nextest
fi

# --- shellcheck (scripts/ci.sh gates on it) --------------------------------
if have shellcheck; then
  log "shellcheck present: $(shellcheck --version | awk '/^version:/ {print $2}')"
else
  log "shellcheck (package manager)"
  pkg_install shellcheck || echo "setup: shellcheck not installed; ci.sh will skip that step." >&2
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

if [ -n "$proxy_dir" ]; then
  log "PATH"
  echo "setup: cargo and rustc live in $proxy_dir, which is not on your PATH." >&2
  echo "setup: add it in your shell's startup file (Homebrew: \`brew info rustup\`), open a new shell and re-run make setup." >&2
  exit 1
fi

log "ready"
printf '  %-14s %s\n' rustc "$(rustc --version)" cargo "$(cargo --version)" nextest "$(cargo nextest --version | head -n 1)"
[ "$want_bench" = 1 ] && printf '  %-14s %s\n' hyperfine "$(hyperfine --version)" jq "$(jq --version)"
exit 0
