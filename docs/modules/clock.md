# `clock`

Local wall-clock time with a spinner.

The local time (system zone, or `tz`), preceded by a spinner whose frame is derived from the current second so it advances on every one-second tick without keeping state.

**Sources:** `wall clock`

**Refresh:** every tick (payload only)

## Presets

| preset | render |
|---|---|
| `minimal` | `16:00` |
| `default` | `⠋ 16:00:00` |
| `full` | `⠋ 16:00:00 Sat 01 Feb +00:00` |

## Icon sets (default preset)

| icons | render |
|---|---|
| `nerd` | `⠋ 16:00:00` |
| `unicode` | `⠋ 16:00:00` |
| `emoji` | `🕐 16:00:00` |
| `ascii` | `\| 16:00:00` |

## Options

`[modules.clock]`

| key | type | minimal | default | full | description |
|---|---|---|---|---|---|
| `enabled` | bool | `true` | `true` | `true` | Render this module. |
| `preset` | `minimal` \| `default` \| `full` | — | — | — | Which preset the options below default to. |
| `refresh` | integer | `0` | `0` | `0` | Seconds between background refreshes; 0 = every tick. |
| `hide` | list of `empty` | `[]` | `[]` | `[]` | States that hide the module: `empty` is what `hide_when_empty` hides, and the two combine. |
| `label` | string ≤ 4096 chars | `""` | `""` | `""` | Dim text before the value. |
| `prefix` | string ≤ 4096 chars | `""` | `""` | `""` | Text before the module. |
| `suffix` | string ≤ 4096 chars | `""` | `""` | `""` | Text after the module. |
| `hide_when_empty` | bool | `true` | `true` | `true` | Hide the module when it has nothing to show (else a dim `–`). |
| `max_width` | integer ≤ 1024 | `0` | `0` | `0` | Cut the whole module (label, prefix and suffix included) to this many cells with `…`, before alignment and before the line is cut; 0 = unlimited. |
| `format` | `24h` \| `12h` | `"24h"` | `"24h"` | `"24h"` | Hour format. |
| `seconds` | bool | `false` | `true` | `true` | Show seconds. |
| `spinner` | bool | `false` | `true` | `true` | Show the spinner. |
| `date` | bool | `false` | `false` | `true` | Show the date. |
| `utc_offset` | bool | `false` | `false` | `true` | Show the UTC offset. |
| `tz` | string | `""` | `""` | `""` | Time zone, read as `TZ` is: an IANA name (`Europe/Berlin`), a POSIX rule (`JST-9`) or a TZif path; empty means the system zone. |

## Icons

`[modules.clock.icons]`

| key | nerd | unicode | emoji | ascii | description |
|---|---|---|---|---|---|
| `spinner` | `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` | `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` | `🕐🕑🕒🕓🕔🕕🕖🕗🕘🕙🕚🕛` | `|/-\` | Spinner frames, one character each, cycled one per tick; `spinner_frames = [...]` is the general form (SPEC § 4.2) and takes strings of any one width. |

Also try (`spinner`: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏` `|/-\` `▁▃▅▇█▇▅▃` `⣾⣽⣻⢿⡿⣟⣯⣷` `⠁⠂⠄⡀⢀⠠⠐⠈`).

Any icon key also accepts `<key>_frames = ["…", "…"]`: glyphs of one width cycled one per tick (frame = `floor(now) mod n`); with `animate = false` frame 0 shows. See [Animation](../guide.md#animation).


## Colors

`[modules.clock.colors]` — a theme role or a literal color (`red`, `208`, `#ff8800`).

| key | default | description |
|---|---|---|
| `spinner` | `accent` | Spinner. |
| `time` | `text` | Time. |
| `date` | `muted` | Date and offset. |
