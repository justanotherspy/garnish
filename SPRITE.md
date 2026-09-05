# SPRITE.md — working on garnish from a Sprite

A Sprite is an isolated, persistent cloud VM driven through the Sprites MCP
tools and the `sprites-*` skills. garnish was bootstrapped on one. This file
is loaded into context by the SessionStart hook **only** when
`SESSION_HOST=sprite`; it adds the Sprite-specific answers to "where is X"
and "how do I do Y here" on top of `CLAUDE.md`, which stays host-neutral.

## Layout and tools

- Rust lives under `/.sprite/languages/rust`; `cargo install` puts binaries
  in `/.sprite/languages/rust/cargo/bin`, not `~/.cargo/bin`. Both are on
  `PATH`.
- `make setup` (`scripts/setup.sh`) installs or updates the toolchain and
  `cargo-nextest`; `make setup ARGS=--all` adds `hyperfine`, `jq` and
  `watchexec`. It uses `cargo install --locked` here.
- rustup prints `can't determine memory limit: sysinfo failure` on every
  invocation. Harmless; ignore it.
- `make watch` writes to `target/watch.log`, which the Monitor tool can
  follow.

## GitHub access

- `gh` is **not** authenticated on a Sprite. Every GitHub API call (opening a
  PR, reading check runs) goes through the Sprites GitHub gateway: invoke
  the `sprite-api-gateway` skill.
- `origin` is `git@github.com:justanotherspy/garnish.git`, reached with the
  Sprite's own key `~/.ssh/id_ed25519` (registered on the account as
  `sprite-garnish`).
- Actions job logs sit behind a redirect the gateway does not follow, so a
  failed run is read from its check-run annotations instead:
  `GET /repos/justanotherspy/garnish/check-runs/<job id>/annotations`.
  `scripts/ci-annotate.sh` keeps those populated.

## Git identity on the Sprite

The repo-local git config on the Sprite is:

```
gpg.format      = ssh
commit.gpgsign  = true
user.signingkey = ~/.ssh/id_ed25519
user.email      = 4822513+justanotherspy@users.noreply.github.com
```

The noreply address is what makes GitHub credit the commits to
`justanotherspy` (the Sprite's default identity maps to a different
account). If GitHub shows a Sprite commit as *unverified*, the Sprite's key
is not registered as a signing key on the account; that is fixed in the
account's settings by the user, not in this repository. Never commit with
`--no-gpg-sign`.

## Checkpoints

A Sprite checkpoint snapshots the whole VM and is the only backup of
anything not yet pushed, and a snapshot can be corrupt (it happened on
2026-09-04). So the order is fixed:

```
make check
git commit -S
git push
sprite-env checkpoints create --comment "garnish: <what landed>"
```

Take one after every phase, after every review-fix batch, and after any hour
of work. The `sprites-checkpoint` skill lists, inspects and restores
checkpoints. After a restore, run `git status` and
`git log origin/main..HEAD` first: the tree may be behind or ahead of what
was pushed.
