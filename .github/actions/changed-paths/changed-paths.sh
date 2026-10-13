#!/usr/bin/env bash
#
# Says whether a pull request changes a file that matches one of a list of patterns. The answer,
# `true` or `false`, is the only thing on stdout; everything else goes to stderr. The composite
# action next to this script (action.yml) runs it and turns the answer into an output.
#
#   EVENT_NAME=pull_request PR_BASE_SHA=<sha> PR_HEAD_SHA=<sha> PATTERNS=<patterns> \
#       .github/actions/changed-paths/changed-paths.sh
#
# It reads the git repository in the current directory, which has to hold both commits and the
# history between them (a full clone, not a shallow one). The environment:
#
#   PATTERNS     One shell pattern per line. A pattern is matched against the whole path of a
#                changed file the way bash matches `[[ $path == $pattern ]]`, so `*` also matches
#                `/` and `android/*` is everything under `android/`. Blank lines, lines starting
#                with `#` and the spaces around a line are ignored. At least one pattern is
#                required, whatever the event.
#   EVENT_NAME   The event that started the workflow. Anything but `pull_request` is answered `true`
#                without looking at the commits or the repository: a push, a manual run and a call
#                from a release workflow always run everything. GitHub names the event of every
#                run, so the name is required.
#   PR_BASE_SHA  The commit the pull request is based on.
#   PR_HEAD_SHA  The last commit of the pull request.
#
# The patterns are read and checked before the event is looked at. No pattern at all is an error,
# since it can only be a mistake, and a workflow with none should fail on its first run, whatever
# started it, and not on its first pull request. No event name is an error for the same reason:
# without one every run looks like one that is not a pull request, and everything would run on
# every pull request without a word.
#
# The changed files are the ones git lists for `<base>...<head>`, which is what the pull request
# changes and not what its base branch has gained since. A rename is listed as the deletion of the
# old path and the addition of the new one, so that either path can match, and the list is read
# NUL-separated, so that no file name can break it.
#
# When the changed files cannot be found, because a commit is missing, a temporary file cannot be
# made for the list or git fails to list them, the answer is `true`: running a check that was not
# needed is harmless, skipping one that was needed is not. That answer alone would hide that the
# filter has stopped working, after a typo in the name of a variable that action.yml sets for the
# commits, for example: everything would run on every pull request and nothing would fail. So these
# three cases are also shown as a warning on the run. Their message is printed as a workflow
# command, `::warning::changed-paths: <text>`, which only counts at the start of a line. Every
# other message is a plain line, `changed-paths: <text>`.
set -euo pipefail

# A plain line of the log.
say() {
    echo "changed-paths: $*" >&2
}

# A line of the log that GitHub also shows as a warning on the run. Like every other message it
# goes to stderr, since stdout holds the answer.
warn() {
    echo "::warning::changed-paths: $*" >&2
}

patterns=()
while IFS= read -r line; do
    # Drop the white space around the line, the CR of a CRLF line end included.
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"
    case "$line" in
        '' | '#'*) continue ;;
    esac
    patterns+=("$line")
done <<<"${PATTERNS:-}"

if [[ "${#patterns[@]}" -eq 0 ]]; then
    say "no patterns were given, which can only be a mistake."
    exit 1
fi

if [[ -z "${EVENT_NAME:-}" ]]; then
    say "the name of the event is missing, which can only be a mistake."
    exit 1
fi

if [[ "$EVENT_NAME" != pull_request ]]; then
    say "the event is '$EVENT_NAME', not a pull request, so everything runs."
    echo true
    exit 0
fi

# An empty side of `...` stands for HEAD, so a missing commit has to be caught here.
if [[ -z "${PR_BASE_SHA:-}" || -z "${PR_HEAD_SHA:-}" ]]; then
    warn "the base or head commit of the pull request is missing, so a changed file is assumed to" \
        "match."
    echo true
    exit 0
fi

# A file rather than a variable or a pipe: the list holds NUL bytes, and the status of git has to
# be seen.
if ! changed_files="$(mktemp "${TMPDIR:-/tmp}/changed-paths.XXXXXX")"; then
    warn "could not create a temporary file for the list of changed files, so a changed file is" \
        "assumed to match."
    echo true
    exit 0
fi
trap 'rm -f "$changed_files"' EXIT

if ! git diff --no-renames --name-only -z "$PR_BASE_SHA...$PR_HEAD_SHA" -- >"$changed_files"; then
    warn "could not list the files changed between $PR_BASE_SHA and $PR_HEAD_SHA, so a changed" \
        "file is assumed to match."
    echo true
    exit 0
fi

count=0
while IFS= read -r -d '' path; do
    count=$((count + 1))
    for pattern in "${patterns[@]}"; do
        # shellcheck disable=SC2053 # The pattern is a glob on purpose.
        if [[ "$path" == $pattern ]]; then
            # Quoted, so that a file name with a newline stays on one line and cannot start a line
            # with `::`, which GitHub would take for a workflow command.
            say "the changed file $(printf '%q' "$path") matches '$pattern'."
            echo true
            exit 0
        fi
    done
done <"$changed_files"

say "none of the $count changed files matches any of the ${#patterns[@]} patterns."
echo false
