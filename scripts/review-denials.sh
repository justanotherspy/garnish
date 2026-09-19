#!/usr/bin/env bash
# Report what the Claude review was refused, from the action's execution file.
#
#   ./scripts/review-denials.sh "$EXECUTION_FILE"
#
# The action prints a sanitised result line to the job log that carries
# `permission_denials_count` and nothing else, so three reviews in a row ended
# having posted nothing and the tool surface had to be *inferred* from the
# allowlist each time (PR #66, and PR #69 twice). The full message stream is
# written to disk regardless of `show_full_output`, and it carries the denied
# calls themselves, so this reads that file and names them.
#
# Prints a markdown report on stdout and exits 1 when anything was denied: a
# denial is always an allowlist bug, and a review that spends its budget being
# told no must not leave a green check behind (CLAUDE.md § Claude review).
#
# A denied command is printed as its verb only: the command word, plus the
# subcommand words that follow it when the command is one that takes them
# (`git rev-parse`, `gh pr diff`). That is what the allowlist matches on, so it
# is all that is needed to write the missing entry, and it keeps an argument
# that might carry a path or a token out of a public job log.
set -uo pipefail
file="${1:?usage: review-denials.sh EXECUTION_FILE}"

if ! command -v jq > /dev/null 2>&1; then
  echo "### Claude review: cannot read the execution file"
  echo
  echo "No \`jq\` on this runner, so the run's tool use was not inspected."
  exit 0
fi

if [ ! -s "$file" ]; then
  echo "### Claude review: no execution file"
  echo
  echo "The action wrote no execution file (\`$file\`), so the run's tool use"
  echo "cannot be read. The run most likely died before Claude started."
  exit 0
fi

# The last result message is the run's verdict; everything else is transcript.
result="$(jq -c '[.[] | select(.type == "result")] | last // {}' "$file")" || {
  echo "### Claude review: unreadable execution file"
  echo
  echo "\`$file\` is not the JSON array the action is expected to write."
  exit 0
}

field() { printf '%s' "$result" | jq -r "$1"; }

turns="$(field '.num_turns // "?"')"
subtype="$(field '.subtype // "?"')"
is_error="$(field '.is_error // false')"
cost="$(field 'if .total_cost_usd then ((.total_cost_usd * 100 | round) / 100 | tostring) else "?" end')"
denials="$(field '(.permission_denials // []) | length')"

# Every posting route the prompt tells the review to use, so "it ran but said
# nothing" is visible without opening the pull request.
attempts() {
  jq -r "$1 | length" "$file"
}
inline="$(attempts '[.[] | select(.type == "assistant") | .message.content[]?
  | select(.type == "tool_use")
  | select(.name == "mcp__github_inline_comment__create_inline_comment")]')"
summary="$(attempts '[.[] | select(.type == "assistant") | .message.content[]?
  | select(.type == "tool_use") | select(.name == "Bash")
  | select((.input.command // "") | startswith("gh pr comment"))]')"

echo "### Claude review: tool use"
echo
echo "| | |"
echo "|---|---|"
echo "| result | \`$subtype\` (is_error: \`$is_error\`) |"
echo "| turns | $turns |"
echo "| cost | \$$cost |"
echo "| denied calls | $denials |"
echo "| summary comments posted | $inline inline, $summary top-level |"
echo

# Red on either of the two ways a review wastes its whole budget, because the
# job's own conclusion says nothing about them: the action reports success
# whenever Claude returned a result, however little it did with it.
fail=0

if [ "$summary" = "0" ]; then
  echo "> **The review posted no summary.** Whatever else it found is lost:"
  echo "> the checkout and the transcript go away with the runner."
  echo
  fail=1
fi

if [ "$denials" = "0" ]; then
  echo "Nothing was refused."
  exit "$fail"
fi

echo "#### What was refused"
echo
echo "Grouped by the key the allowlist matches on. Add the missing entries to"
echo "\`--allowedTools\` in \`.github/workflows/claude-review.yml\`, or tell the"
echo "review in its prompt not to reach for them."
echo
echo '```'
printf '%s' "$result" | jq -r '
  # A word that can be a subcommand: no slash, no dot, no leading dash, so a
  # path or a flag ends the verb rather than being printed as part of it.
  def verbish: . != null and (. | test("^[a-z][a-z0-9_-]*$"));
  # Only these take subcommands; for anything else the command word is the key.
  def drives: test("^(git|gh|cargo|make|npm|bun|docker|node|python3?)$");
  (.permission_denials // [])
  | map(
      if .tool_name == "Bash" then
        (
          (.tool_input.command // "")
          | gsub("\\s+"; " ")
          | ltrimstr(" ")
          | split(" ")
        ) as $w
        | ($w[0] // "") as $c
        | (
            if $c == "" then "Bash"
            elif ($c | drives | not) then "Bash(" + $c + ":*)"
            elif ($w[1] | verbish | not) then "Bash(" + $c + ":*)"
            elif ($w[2] | verbish) then "Bash(" + $c + " " + $w[1] + " " + $w[2] + ":*)"
            else "Bash(" + $c + " " + $w[1] + ":*)"
            end
          )
      else
        .tool_name
      end
    )
  | group_by(.)
  | map({ key: .[0], n: length })
  | sort_by(-.n, .key)
  | .[]
  | "\(.n)\t\(.key)"
'
echo '```'

# A command can be refused for its shape rather than its verb: a pipe into
# something the allowlist does not carry refuses the whole line, and so does a
# flag before the subcommand, since the allowlist matches on a prefix and
# `git --no-pager diff` does not start with `git diff`. Grouped by verb those
# are invisible - run 35452476655 reported four refused `Bash(git diff:*)`
# calls while `Bash(git diff:*)` was in the allowlist. So name every verb a
# denied command ran. Verbs only, never arguments, same as above.
shapes="$(
  printf '%s' "$result" | jq -r '
    (.permission_denials // [])
    | map(select(.tool_name == "Bash") | (.tool_input.command // ""))
    | map(
        gsub("\\s+"; " ")
        | [ splits("\\|\\||&&|[|;&]") ]
        | map(ltrimstr(" ") | split(" ") | .[0] // "")
        | map(select(. != ""))
        | join(" → ")
      )
    | map(select(test(" → ")))
    | unique
    | .[]
  '
)"

# `$(…)`, a backtick and a redirect do not split on an operator, so a command
# refused for carrying one looks single and innocent above. Name the construct
# instead of the command.
constructs="$(
  printf '%s' "$result" | jq -r '
    (.permission_denials // [])
    | map(select(.tool_name == "Bash") | (.tool_input.command // ""))
    | map(
        [ (select(test("\\$\\(")) | "$( … )"),
          (select(test("`")) | "` … `"),
          (select(test("\\$\\{")) | "${ … }"),
          (select(test("[0-9]?>>?[^|]")) | "redirect") ]
      )
    | flatten
    | group_by(.)
    | map("\(length)\t\(.[0])")
    | .[]
  '
)"

if [ -n "$shapes" ]; then
  echo
  echo "#### Refused as a whole line"
  echo
  echo "Every verb these denied commands ran. A verb that is already allowed"
  echo "on its own means the line died on the company it kept, not on itself."
  echo
  echo '```'
  printf '%s\n' "$shapes"
  echo '```'
fi

if [ -n "$constructs" ]; then
  echo
  echo "#### Shell constructs among the denials"
  echo
  echo "These do not split on an operator, so the commands carrying them look"
  echo "single above. One command per call, with no substitution, is the rule."
  echo
  echo '```'
  printf '%s\n' "$constructs"
  echo '```'
fi

exit 1
