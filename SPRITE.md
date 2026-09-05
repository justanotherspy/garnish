# SPRITE.md — building garnish on a Sprite

garnish was bootstrapped on a Sprite (an isolated, persistent cloud VM
driven through the Sprites MCP tools and the `sprites-*` skills) and most of
its history was written there. This file holds everything about *that
environment* so `CLAUDE.md` can stay host-neutral. Read `CLAUDE.md` first;
this note only adds the Sprite-specific answers to "where is X" and "how do
I do Y here".

## Layout and tools

- Rust lives under `/.sprite/languages/rust`. `cargo install` puts binaries
  in `/.sprite/languages/rust/cargo/bin`, not `~/.cargo/bin`; both are on
  `PATH`. That is where `cargo-nextest` and `hyperfine` are.
- The toolchain is the rolling nightly from `rust-toolchain.toml`;
  `rustup toolchain install` in the repo (re)installs it with the pinned
  components.
- rustup prints `can't determine memory limit: sysinfo failure` on every
  invocation. Harmless; ignore it.
- `watchexec` is installed, so `make watch` works and writes to
  `target/watch.log`, which the Monitor tool can follow.
- There is no `devup` here. Missing tools are installed with `cargo install`
  (or the distro package manager) and noted in this file.

## GitHub access

- `gh` is **not** authenticated on a Sprite. Every GitHub API call (opening a
  PR, reading check runs, registering keys) goes through the Sprites GitHub
  gateway: invoke the `sprite-api-gateway` skill. The gateway's token scopes
  are limited; it cannot list or add SSH *signing* keys
  (`admin:ssh_signing_key`).
- `origin` is `git@github.com:justanotherspy/garnish.git`. The Sprite's
  key is `~/.ssh/id_ed25519`, registered on the `justanotherspy` account as
  `sprite-garnish` (2026-09-04) for authentication. As of 2026-09-04 it was
  **not** registered as a signing key: GitHub showed `unknown_key` on the
  Sprite's signed commits. Adding it is a manual step in GitHub's SSH
  signing keys settings (`PLAN.md` backlog).
- Actions job logs sit behind a redirect the gateway does not follow, so a
  failed run is diagnosed from its check-run annotations instead:
  `GET /repos/justanotherspy/garnish/check-runs/<job id>/annotations`.
  `scripts/ci-annotate.sh` exists so that those annotations are always
  populated (one `::error::` per failing test, clippy error or rustfmt diff,
  plus the log tail when nothing matched).

## Git identity on the Sprite

The repo-local git config on the Sprite is:

```
gpg.format      = ssh
commit.gpgsign  = true
user.signingkey = ~/.ssh/id_ed25519
user.email      = 4822513+justanotherspy@users.noreply.github.com
```

Why: GitHub attributes commits by author email, and the Sprite's default
identity (the hey.com address) belongs to a different GitHub account, so
early commits were credited to the wrong user. The noreply address always
maps to `justanotherspy`. GitHub verifies an SSH signature only when the
key is also registered as a *signing* key on the account (see above: not
yet, as of 2026-09-04). To re-check after the key is added, push a signed
commit and read `commit.verification.reason` from the commits API
(`GET /repos/justanotherspy/garnish/commits/<sha>`): `valid` means
registered, `unknown_key` means not. The repository ruleset on `main`
requires signed commits. Never work around a failing signature with
`--no-gpg-sign`; report it.

## Checkpoints

A Sprite checkpoint snapshots the whole VM and is the only backup of
anything not yet pushed. The VM has been restored from a corrupt snapshot
once (2026-09-04) and uncommitted work survived only by luck, so the order is
fixed:

```
make check
git commit -S
git push
sprite-env checkpoints create --comment "garnish: <what landed>"
```

Take one after every phase, after every review-fix batch, and after any hour
of work. The `sprites-checkpoint` skill lists, inspects and restores
checkpoints. After a restore, run `git status`
and `git log origin/main..HEAD` first: the tree may be behind or ahead of
what was pushed.

## Claude Code on the Sprite

- The rust-analyzer LSP plugin is installed in the Sprite's Claude Code, so
  the LSP tools in `CLAUDE.md` § Session protocol are available.
- CI triage goes through the gateway and the annotations route above unless
  the `shuck` CLI has been installed since; if it has, note it here.

## History

- 2026-09-04: project scaffolded here; phases 0–9 and `v0.1.0` built in one
  day; pushed to GitHub through the gateway; first CI runs and macOS fixes;
  VM restored from a corrupt snapshot in the evening.
- 2026-09-05: work continues on Daniel's own machine as well (see the
  `PLAN.md` session log); this file was split out of `CLAUDE.md`.
