# GARLIC-INTEGRATION.md — garnish as garlic's per-second sensor (proposal)

Status: a proposal for review, written 2026-09-06. Nothing here is in
`SPEC.md` yet and nothing is implemented. It answers one question from the
maintainer: garnish runs once a second in every open Claude Code session
with that session's numbers on stdin; could that tick track how long the
user has actually been working, notice which session has the user's
attention, and hand [garlic](https://github.com/justanotherspy/garlic) a
better signal than its hook-based estimate, with garlic reduced to a relay
that delivers the nudges?

The short answer: **yes, but not as a replacement**. The tick adds three
things garlic cannot see today (a liveness heartbeat, an API-time lens, and a
visible counter in the status line). It does not see the events garlic
already timestamps exactly, and it cannot see terminal focus at all. The
recommended shape keeps garlic as the engine and the relay, makes garnish the
sensor, and adds a status line module that shows garlic's day. The variant
the question asked for (garnish owns the tracking and the nudge policy) is
written out in full in § 6.2 so it can be chosen against this recommendation.

## 1. What each tool sees today

| | garlic (hooks) | garnish (status line tick) |
|---|---|---|
| runs | on `SessionStart`, `UserPromptSubmit`, `Stop`, `SessionEnd` | every `refreshInterval` seconds (min 1) plus on every state change (300 ms debounce), while the session's TUI is open |
| input | hook JSON: `session_id`, event, timestamp | the status line payload (SPEC § 2.2): `session_id`, `prompt_id`, `cost.total_duration_ms`, `cost.total_api_duration_ms`, token counts, `transcript_path`, model, cwd |
| state | `~/.garlic/state.toml` under `flock`, one file for the whole machine: closed intervals per session, open cursors, thresholds given, `ignored`, history, `nudge_pending` | per-session directory `<cache>/sessions/<session_id>/` (SPEC § 6), written only by refresh workers; the render tick reads and never writes |
| model | Prompt→Stop is **agent** time (capped at `max_generation_minutes`), Stop→Prompt is **user** time (dropped over `max_prompt_gap_minutes`); the day is the **union** across sessions | stateless: every render is a pure function of payload, config and clock (SPEC § 4.2) |
| output | `additionalContext` on the next prompt (the framed nudge), `garlic status`, `garlic statusline`, an optional shared backend | the status line rows |
| covers | every session that runs hooks, including `claude -p` and SDK sessions | only sessions with a TUI and a configured `statusLine` |

Two facts about garlic's engine matter for what follows (read from
`src/engine.rs` and `src/state.rs` on 2026-09-06):

- An open cursor closes only when its own session fires the next event. A
  session whose terminal is killed (no `SessionEnd`) leaves its cursor open
  until the day rolls over, and the rollover starts a fresh state, so **the
  in-flight span is lost, not inflated**. The `max_generation_minutes` cap
  protects only against a hung generation that does eventually stop.
- Prompt→Stop counts as agent time whether the model was generating, a tool
  was running, or the harness was waiting for the user to answer a
  permission prompt. Those are three different things to a person watching
  the screen, and only the first is "the agent working".

## 2. Where a one-second sampler adds nothing

garlic's hooks carry exact timestamps for the prompt and stop boundaries.
A sampler running once a second sees the same boundaries up to a second
late (it notices `prompt_id` changed at its next tick). So for the numbers
garlic already computes, the tick does not make the total more accurate;
it makes it slightly less so. Any design that throws the hook timestamps
away in favour of samples is a step backwards.

The tick also has holes the hooks do not: nothing runs in `claude -p`, in
SDK sessions, in a session without `statusLine`, or while the harness has
cancelled an in-flight render (a new trigger cancels the running script).
garnish cannot be the only sensor.

## 3. What the sampler does add

1. **Liveness.** A session whose payload keeps arriving is alive. A session
   whose last sample is older than a couple of refresh intervals is gone,
   whether or not `SessionEnd` fired. garlic can close that session's
   cursor at the time of the last sample instead of losing the span.
2. **API time.** `cost.total_api_duration_ms` grows only while a request is
   in flight. Its delta between samples splits a Prompt→Stop span into model
   time versus tool runs and permission waits. That is a new facet for
   `garlic status` ("agent 41m, of which the model was generating 12m"), not
   a change to the daily total.
3. **A visible counter.** The status line is on screen in every session all
   day. Today's total, the distance to the next threshold, and the moment a
   nudge fires can be shown there with no relay through Claude at all. This
   is the largest new value and needs none of the tracking below.
4. **Tokens per session.** The context and token counts let a day be broken
   down by how much was read and generated, which garlic has no window on.

## 4. Focus: what the harness knows and what it exposes

Read from the Claude Code 2.1.263 binary on 2026-09-06 (`strings`, then
searching for `terminalFocus`, `refreshInterval`, `exceeds_200k_tokens`):

- The harness **does track terminal focus**. It enables the terminal's
  focus-event mode (the same DEC mode tmux's `focus-events` option forwards,
  which the binary probes), keeps a `terminalFocus` state of `focused`,
  `blurred` or `unknown` with `terminalFocusGainedAt`, and a
  `lastInteractionTime` from keyboard and scroll activity. Its presence rule
  is: focus when known, else an interaction within the last 60 s.
- That presence is used for one thing: suppressing push notifications while
  the user is present (`CLAUDE_CODE_DISABLE_NOTIFICATION_PRESENCE_CHECK`
  turns the check off). It is **not** in the status line payload (the
  builder emits the fields SPEC § 2.2 lists, plus a `remote.session_id` for
  remote sessions that SPEC should pick up) and there is **no hook event**
  for focus; the hook list is `SessionStart`, `SessionEnd`,
  `UserPromptSubmit`, `Stop`, `SubagentStart/Stop`, `PreToolUse`,
  `PostToolUse`, `PermissionRequest`, `Notification`, `PreCompact`,
  `TeammateIdle`, `TaskCreated/Completed`, `Setup`, `WorktreeCreate/Remove`,
  `StopFailure`, `UserPromptExpansion`.

So the honest options are:

1. **Ask upstream** for a `presence` object in the status line payload
   (`terminal_focus`, `last_interaction_ms`). The harness already has both
   numbers; the change is a few lines in the payload builder. Exact, cheap,
   and the only way to real focus. Worth an issue on
   `anthropics/claude-code`.
2. **Query the OS** for the active window and walk it to the terminal that
   owns this session. Rejected: it differs between X11, every Wayland
   compositor and macOS, needs a new dependency or a child process on the
   warm tick (SPEC § 6 forbids the latter), and still has to map a window to
   a pid to an ancestor of the status line process.
3. **Define focus as activity.** The session the user is "in" is the one
   whose `prompt_id` changed most recently; among sessions with no prompt
   since the last sample, the one whose API time is growing is the one being
   waited on. This is what garlic's union already assumes, expressed per
   second. It cannot tell "reading session B's answer" from "went for
   coffee"; nothing without focus can.

Recommendation: 3 now, 1 as the ask. Do not leave 2 as a maybe.

## 5. The tick cadence (verified, with one thing left to check)

The status line re-runs on a timer of `max(1, refreshInterval) × 1000` ms
"in addition to event-driven updates" (the schema's own description), the
events being changes to token usage, permission mode, vim mode, model, fast
mode, effort, thinking and PR status, plus scheduled re-renders when a
rate-limit reset or the prompt cache expiry passes. No focus or idleness
gate appears in that timer. **To verify empirically**: that a blurred
terminal keeps ticking at `refreshInterval` (leave a session unfocused with
`GARNISH_DEBUG` on and read the timestamps). If it does not, liveness in
§ 3 becomes "the session is alive *and focused*", which is better, not
worse, for this purpose.

## 6. Three shapes

### 6.1 garnish is the sensor, garlic stays the engine and the relay (recommended)

- garnish writes one **activity file per session** (§ 7) from the render
  tick: a sample when `prompt_id` or `cost.total_api_duration_ms` changed,
  else a heartbeat at most every 30 s. Nothing is aggregated in garnish.
- garlic's existing hooks, which already fire on every prompt and stop,
  read the activity files of the sessions they know about and:
  - close the cursor of any session whose last sample is older than a
    liveness window, at that sample's time;
  - record the API-time facet on the agent interval they are closing.
- `garlic status` gains the facet; thresholds, styles, bedtime, `ignore`,
  `reset`, history, week/month views and the backend are untouched. The
  relay stays `frame_nudge` on `UserPromptSubmit`.
- Optionally `garnish activity --json` prints the machine's live sessions
  (id, last sample age, prompt count, API minutes) for `garlic status` and
  for people.

What it costs garnish: SPEC § 4.2's "no state between ticks" stays true for
rendering, but SPEC § 6's "the tick never writes" gets one exception (a
write at most every 30 s per session, temp file + rename). The per-session
directory already exists and is already garbage-collected after 24 h idle.
Estimated cost per tick: one `stat` and a read of a file under 1 KB when
nothing changed, well under 0.1 ms against the 3 ms budget; the write path
runs on a change or a heartbeat only.

### 6.2 garnish owns the tracking and the nudge policy; garlic relays (the question as asked)

- `garnish.toml` gains a `[nudge]` table: `thresholds_minutes`, `style`,
  `max_prompt_gap_minutes`, `reset_hour`, `bedtime`.
- Every tick reads its own session's activity file and a **machine-wide
  state file** under a lock (the union of all sessions' intervals, the
  thresholds given today, the ignore flag), updates it, and when a threshold
  is crossed writes a **nudge signal** (§ 7.3) that any hook can read.
- garlic's `UserPromptSubmit` hook checks the signal file and emits its
  framed nudge; nothing else in garlic runs.

What this duplicates: garlic's entire engine (intervals, union, gaps and
caps, thresholds given once, bedtime window, ignore, reset, daily rollover,
history) and its config. What it orphans: `garlic status --week/--month`,
`garlic sync` and the backend that merges intervals across machines, unless
garnish re-implements the merge protocol too. What it does not remove:
garlic's hook. The only path into Claude's context is a hook's
`additionalContext`, so garlic stays installed either way; it becomes a
thin shell around a file read. And the machine-wide state file with locking
is exactly the thing SPEC § 6 designed garnish's cache to avoid (a render
tick that takes a lock can stall on another tick).

This shape makes sense only if garlic's engine is going to be retired and
its backend dropped. If that is the plan, the split below (garnish reads
and writes the same `state.toml` format under garlic's `flock`) is the
cheaper route than a second format.

### 6.3 The display path: a status line module for garlic's day

Independent of 6.1 and 6.2, and the first thing to build:

- A `garlic` module (cached, `refresh = 30`, SPEC § 6 worker) whose worker
  runs `garlic status --json` and stores today's total, the highest
  threshold, the agent/user split, `ignored` and `nudge_pending`. The tick
  renders `⏳ 2h10m / 4h` with the band colours the other percentage modules
  use, the garlic glyph while a nudge is fresh, `(paused)` when ignored. No
  garlic on the PATH: the module renders nothing (a failed entry is fresh
  for its TTL, so a missing binary is probed once per TTL, not per tick).
- Why a worker and not a direct read of `state.toml`: no coupling to
  garlic's file format or its lock, no I/O on the warm tick, and `garlic
  status --json` is the interface garlic already promises for "scripting and
  statusline integrations".
- Spec impact: the module set is fixed at 21 ids (SPEC § 3, CLAUDE.md);
  adding one is a SPEC change that needs the maintainer's OK. It is the
  first module that runs a non-git program, so `git::run_program`'s
  kill-on-timeout is the way to run it (2 s), as for every subprocess.

**Recommendation: 6.3 first, then 6.1. Revisit 6.2 only if garlic's engine
is being retired.** 6.3 is pure display and pays off immediately. 6.1 is
small on both sides and fixes a real loss (the killed session). 6.2 moves a
working engine into a tool built to be stateless and keeps garlic anyway.

## 7. The contract

Both repositories belong to the same maintainer, so the file formats are the
thing to review here. Everything is line-oriented text, versioned on its
first line, written as `<file>.tmp.<pid>` then renamed, read with the rule
"a malformed line is skipped, an unknown version is ignored".

### 7.1 Activity file (garnish writes, garlic reads)

`<garnish cache root>/sessions/<session_id>/activity`, in the directory
garnish already sanitises and garbage-collects. The cache root is resolved
as SPEC § 6 says (`GARNISH_CACHE_DIR`, then `$XDG_RUNTIME_DIR/garnish`, and
so on), so garlic must resolve it the same way; garnish should print it
(`garnish doctor` already does, `garnish activity --path` would be the
scriptable form).

```
v1 <session_id> <first_seen_ms> <cwd>
<ts_ms> <prompt_id or -> <api_ms> <duration_ms> <input_tokens> <output_tokens>
```

- Line 1 identifies the file. The remaining lines are samples, newest last,
  appended on a change of `prompt_id` or `api_ms`, else every 30 s. The
  file is rewritten to its last 200 samples when it passes 1000 lines.
- The file's mtime is the heartbeat; readers need not parse it to know the
  session is alive.
- `cwd` is the only string in it besides ids, and garnish writes it plain
  (SPEC § 5 sanitising applies to everything it writes, not only rows).

### 7.2 What garlic derives (6.1)

- session alive: mtime younger than `2 × refreshInterval + 5 s` (garnish
  writes the interval it was told into line 1 if that becomes useful).
- agent interval: samples where `api_ms` grew; user interval: from the last
  growth to the next `prompt_id` change, dropped past `max_prompt_gap`.
- cursor closing: a session with an open cursor and a dead heartbeat is
  closed at its last sample time.

### 7.3 Nudge signal (6.2 only)

`$XDG_RUNTIME_DIR/garlic/nudge` (mode 0600 in a 0700 directory), one line:

```
v1 <ts_ms> <threshold_minutes> <style> <final:0|1>
```

The file carries a **level, never a message**. The hook that relays it
renders the text from its own hardcoded strings, as garlic's `frame_nudge`
does today. A file that any local process can write must not be able to put
words into Claude's context.

## 8. Security and privacy

- **Injection path.** A hook's `additionalContext` lands in the model's
  context. Nothing read from a file may be relayed verbatim; the signal
  format above is numbers and an enum for that reason, and garlic's relay
  wrapper stays fixed text.
- **File placement.** Runtime directories are per-user and 0700 by
  convention; garnish's cache root falls back to `~/.cache/garnish`, which
  is 0755 on many systems, so the activity files should be created 0600.
- **Symlinks.** Both writers refuse to follow a symlink at the target path
  (garnish's `skills install` already does this; the same check applies).
- **Contents.** The activity file holds timestamps, ids, token counts and
  the working directory. No prompt text, no model output, no transcript
  path. `garnish doctor`'s `~` collapsing applies if the file is ever
  printed.
- **Session ids** are opaque tokens from the harness and are already
  sanitised before becoming a path segment.

## 9. Open questions for the maintainer

1. Is 6.3 (a `garlic` status line module, one more id in the fixed set)
   wanted? It is the smallest step with the most visible result.
2. For 6.1, is a 30 s heartbeat write from the render tick acceptable as
   the one exception to "the tick never writes"? The alternative is a
   refresh worker that samples, which loses the once-per-second resolution
   the idea was built on.
3. Should the upstream ask (a `presence` object in the status line payload)
   be filed now? Without it, "focus" means "activity" in both tools.
4. Does the API-time facet belong in garlic's `status` output, or is it
   display-only in garnish?
5. Is 6.2 still the preferred end state after reading § 6.2? If so, the
   next question is whether garlic's backend is being kept.

## 10. Facts to re-verify after a Claude Code upgrade

- The refresh timer keeps running while the terminal is blurred (§ 5).
- The payload still carries no presence field, and the hook list has no
  focus event (§ 4). Search the binary for `terminalFocus` and
  `hook_event_name:"`.
- `cost.total_api_duration_ms` still grows only during requests, so its
  delta is model time (§ 3).
