#!/usr/bin/env bash
#
# Tests changed-paths.sh, which says whether a pull request changes a file that matches one of a
# list of patterns.
#
#   .github/actions/changed-paths/test.sh
#
# Each case runs the script in a throwaway git repository, with the environment a pull request
# gives it or without one, and compares what the script prints, whether it succeeds and whether its
# log holds a warning annotation, a line that starts with `::warning::`. Nothing in this checkout is
# touched, and nothing beyond bash and git is needed. The whole run takes about two seconds. The
# action (action.yml) runs it as its first step, so that a matcher that does not work fails the job
# that uses it instead of quietly answering `false`.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$HERE/changed-paths.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

REPO="$WORK/repo"
# A hash that no repository holds.
UNKNOWN_SHA="0123456789abcdef0123456789abcdef01234567"

passed=0
failed=0

pass() {
    passed=$((passed + 1))
    echo "  ok    $1"
}

fail() {
    failed=$((failed + 1))
    echo "  FAIL  $1"
    [[ -z "${2:-}" ]] || printf '        %s\n' "${2//$'\n'/$'\n'        }"
}

# What the machine's git setup says (a signing key, hooks, a default branch name, variables of a
# repository this test is run from) must not reach the throwaway repository.
for var in $(git rev-parse --local-env-vars); do
    unset "$var"
done
export HOME="$WORK/home" XDG_CONFIG_HOME="$WORK/home/.config"
export GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
export GIT_AUTHOR_NAME=tester GIT_AUTHOR_EMAIL=tester@example.com
export GIT_COMMITTER_NAME=tester GIT_COMMITTER_EMAIL=tester@example.com
mkdir -p "$HOME"

git_() {
    git -C "$REPO" "$@"
}

# put <path> [text]: writes a file in the repository, and the directories above it.
put() {
    mkdir -p "$(dirname "$REPO/$1")"
    printf '%s\n' "${2:-$1}" >"$REPO/$1"
}

# commit <message>: commits everything and prints the hash of the commit.
commit() {
    git_ add -A
    git_ commit -q -m "$1"
    git_ rev-parse HEAD
}

# from_base: puts the work tree back to the base commit, from where a case makes its own commit.
from_base() {
    git_ checkout -q --detach "$BASE"
}

# run <head> <patterns> [VAR=value...]: runs the script the way a pull request starts it, from
# $BASE to <head>, in $RUN_DIR (the repository unless in_dir says otherwise). The VAR=value
# arguments come last and so win, which is how a case drops or changes a variable. The status goes
# to $status, what the script printed to $WORK/out and $WORK/err.
run() {
    local head="$1" patterns="$2"
    shift 2
    status=0
    (
        cd "${RUN_DIR:-$REPO}"
        env EVENT_NAME=pull_request PR_BASE_SHA="$BASE" PR_HEAD_SHA="$head" \
            PATTERNS="$patterns" "$@" "$BASH" "$SCRIPT"
    ) >"$WORK/out" 2>"$WORK/err" || status=$?
}

# in_dir <directory> <command...>: runs a command whose runs of the script start in <directory>.
in_dir() {
    RUN_DIR="$1"
    shift
    "$@"
    unset RUN_DIR
}

# answer <name> <true|false> <head> <patterns> [VAR=value...]: the script succeeds and prints
# exactly the wanted answer.
answer() {
    local name="$1" want="$2"
    shift 2
    run "$@"
    if [[ "$status" -ne 0 ]]; then
        fail "$name" "exited with status $status: $(<"$WORK/err")"
    elif [[ "$(<"$WORK/out")" != "$want" ]]; then
        fail "$name" "expected $want, got: $(<"$WORK/out")"$'\n'"stderr: $(<"$WORK/err")"
    else
        pass "$name"
    fi
}

# logged <name> <text>: the last run said <text> in the log.
logged() {
    if [[ "$(<"$WORK/err")" == *"$2"* ]]; then
        pass "$1"
    else
        fail "$1" "expected the log to say: $2"$'\n'"got: $(<"$WORK/err")"
    fi
}

# unlogged <name> <text>: the last run did not say <text> in the log.
unlogged() {
    if [[ "$(<"$WORK/err")" != *"$2"* ]]; then
        pass "$1"
    else
        fail "$1" "expected the log not to say: $2"$'\n'"got: $(<"$WORK/err")"
    fi
}

# warned <name>: the last run printed a warning annotation, a line of the log that starts with
# `::warning::changed-paths:`.
warned() {
    local err
    err="$(<"$WORK/err")"
    if [[ $'\n'"$err" == *$'\n::warning::changed-paths:'* ]]; then
        pass "$1"
    else
        fail "$1" "expected a line that starts with ::warning::changed-paths:"$'\n'"got: $err"
    fi
}

# unannotated <name>: the last run printed no line that starts with `::`, so that GitHub shows
# nothing on the run.
unannotated() {
    local err
    err="$(<"$WORK/err")"
    if [[ $'\n'"$err" != *$'\n::'* ]]; then
        pass "$1"
    else
        fail "$1" "expected no line that starts with ::"$'\n'"got: $err"
    fi
}

# refuses <name> <text> <head> <patterns> [VAR=value...]: the script fails, prints no answer and
# says <text> in the log.
refuses() {
    local name="$1" text="$2"
    shift 2
    run "$@"
    if [[ "$status" -eq 0 ]]; then
        fail "$name" "expected a failure, got: $(<"$WORK/out")"
    elif [[ -s "$WORK/out" ]]; then
        fail "$name" "expected no answer, got: $(<"$WORK/out")"
    elif [[ "$(<"$WORK/err")" != *"$text"* ]]; then
        fail "$name" "expected the log to say: $text"$'\n'"got: $(<"$WORK/err")"
    else
        pass "$name"
    fi
}

git -c init.defaultBranch=main init -q "$REPO"
git_ config commit.gpgsign false

# The base commit, and one commit on top of it for each case that needs files changed. A pull
# request is the range from the base to such a commit.
put README.md
put '#notes.txt'
put flake.nix
put flake.nix.bak
put flake.lock
put android/README.md
put android/app/src/Main.kt
put android/old.txt "$(seq 1 20)"
put android/gone.txt
put android-old/x
put docs/README.md
put docs/old-name.txt "$(seq 101 120)"
put ios/Package.swift
put rust/fedimint-sdk/src/lib.rs
BASE="$(commit "base")"

from_base
put android/new.txt
h_android="$(commit "add a file under android/")"

from_base
put android/app/src/main/deep/File.kt
h_android_deep="$(commit "add a file deep under android/")"

from_base
put flake.nix "changed"
h_flake="$(commit "change flake.nix")"

from_base
put flake.nix.bak "changed"
h_flake_bak="$(commit "change flake.nix.bak")"

from_base
put flake.lock "changed"
h_lock="$(commit "change flake.lock")"

from_base
put android-old/x "changed"
h_android_old="$(commit "change android-old/x")"

from_base
put README.md "changed"
h_readme="$(commit "change README.md")"

from_base
put docs/README.md "changed"
h_docs_readme="$(commit "change docs/README.md")"

from_base
put '#notes.txt' "changed"
h_notes="$(commit "change #notes.txt")"

from_base
put rust/fedimint-sdk/src/lib.rs "changed"
h_rust="$(commit "change a file under rust/fedimint-sdk/")"

# Git lists paths in byte order, so the one under rust/ comes last.
from_base
put README.md "changed"
put docs/extra.md
put rust/fedimint-sdk/src/extra.rs
h_several="$(commit "change three files, the last of them under rust/")"

from_base
git_ mv android/old.txt docs/moved.txt
h_rename_out="$(commit "move a file out of android/")"

from_base
git_ mv docs/old-name.txt android/moved-in.txt
h_rename_in="$(commit "move a file into android/")"

from_base
git_ rm -q android/gone.txt
h_delete="$(commit "delete a file under android/")"

from_base
put 'my docs/read me.txt'
h_space="$(commit "add a path with a space")"

from_base
put $'docs/note\nandroid/x'
h_newline="$(commit "add a path with a newline, the second line of which looks like a match")"

from_base
put 'android/quo"te.txt'
h_quote="$(commit "add a path with a double quote")"

from_base
put android/.gitignore
h_dotfile="$(commit "add a dotfile under android/")"

# Paths made of characters that mean something in a pattern. Each is the only change of its commit.
from_base
put 'a*'
h_star="$(commit "add a path with a star")"

from_base
put 'b[1]'
h_bracket="$(commit "add a path with brackets")"

from_base
put 'c?'
h_question="$(commit "add a path with a question mark")"

from_base
put 'd\e'
h_backslash="$(commit "add a path with a backslash")"

from_base
put '--force'
h_dash="$(commit "add a path that starts with a dash")"

# The second line of this name looks like a workflow command.
from_base
put $'note\n::warning::changed-paths: forged'
h_forged="$(commit "add a path with a newline, the second line of which is a workflow command")"

# A head commit whose history has nothing in common with the base commit's, so that the pull request
# has no merge base. It differs from the base commit only in README.md, so that a fallback to a
# comparison of the two trees would answer `false` for `android/*`.
from_base
git_ checkout -q --orphan unrelated
put README.md "changed"
h_unrelated="$(commit "start a history of its own")"

# What the base branch gained after the pull request branched off it.
from_base
put android/main-only.txt
main_moved="$(commit "change android/ on the base branch")"

# What actions/checkout gives without `fetch-depth: 0`: the head commit alone, with no history.
git_ branch -q shallow-source "$h_readme"
git init -q "$WORK/shallow"
git -C "$WORK/shallow" fetch -q --depth 1 "$REPO" shallow-source

# The cases run with the work tree on the base commit, so that a missing commit hash, which git
# reads as HEAD, would give an empty list of changes and not a matching one.
from_base

echo "which files match"

answer "a dir/* pattern matches a file one level down" true "$h_android" 'android/*'
logged "the log says which pattern matched" "matches 'android/*'"
unannotated "a match is a plain line of the log"
answer "a dir/* pattern matches a file deeper down" true "$h_android_deep" 'android/*'
answer "a dir/* pattern matches a dotfile" true "$h_dotfile" 'android/*'
answer "an exact file pattern matches that file" true "$h_flake" 'flake.nix'
answer "a * also matches a /" true "$h_rust" 'rust/*lib.rs'
answer "a ? matches one character" true "$h_lock" 'flake.loc?'
answer "a change to nothing the patterns name does not match" false "$h_readme" \
    $'android/*\nflake.nix\nflake.lock'
unannotated "no match is a plain line of the log"
answer "with several patterns, the last can be the one that matches" true "$h_lock" \
    $'ios/*\nnix/*\nflake.lock'
answer "with several patterns, none of them matching" false "$h_readme" $'ios/*\nnix/*\nflake.lock'
answer "with several changed files, the last can be the one that matches" true "$h_several" \
    'rust/fedimint-sdk/*'
answer "a commit that changes nothing does not match" false "$BASE" '*'

echo
echo "a pattern matches the whole path"

answer "flake.nix does not match flake.nix.bak" false "$h_flake_bak" 'flake.nix'
answer "android/* does not match android-old/x" false "$h_android_old" 'android/*'
answer "README.md does not match docs/README.md" false "$h_docs_readme" 'README.md'
answer "the start of a name does not match without a *" false "$h_flake_bak" 'flake'

echo
echo "the list of patterns"

answer "blank lines are ignored" true "$h_android" $'\n\nandroid/*\n\n'
answer "blank and whitespace-only lines match nothing" false "$h_readme" \
    $'ios/*\n   \n\t\nnix/*\n'
answer "a comment is not a pattern" false "$h_notes" $'#notes.txt\nandroid/*'
answer "an indented comment is not a pattern" false "$h_notes" $'  #notes.txt\nandroid/*'
answer "a pattern after a comment counts" true "$h_android" $'# a comment\nandroid/*'
answer "spaces around a pattern are ignored" true "$h_android" '   android/*   '
answer "a pattern with CRLF line ends counts" true "$h_android" $'ios/*\r\nandroid/*\r\n'
refuses "no pattern at all is an error" "no patterns" "$h_android" ''
refuses "only comments and blank lines is an error" "no patterns" "$h_android" \
    $'\n# android/*\n   \n'

echo
echo "renames, deletions and odd names"

answer "a rename changes the old path" true "$h_rename_out" 'android/*'
answer "a rename changes the new path" true "$h_rename_out" 'docs/*'
answer "a rename into a pattern changes the new path" true "$h_rename_in" 'android/*'
answer "a rename into a pattern changes the old path" true "$h_rename_in" 'docs/*'
answer "a rename that touches no pattern does not match" false "$h_rename_out" 'ios/*'
answer "a deleted file changes its path" true "$h_delete" 'android/*'
answer "a deleted file that no pattern names does not match" false "$h_delete" 'ios/*'
answer "a path with a space matches a pattern with the same space" true "$h_space" \
    'my docs/read me.txt'
answer "a path with a space matches a pattern for its directory" true "$h_space" 'my docs/*'
answer "a path with a space is one path, not two" false "$h_space" 'docs/*'
answer "a path with a newline matches a pattern for its start" true "$h_newline" 'docs/*'
answer "a path with a newline is one path, not two" false "$h_newline" 'android/*'
answer "a path with a newline matches a pattern that spans it" true "$h_newline" '*android/x'
answer "a path with a double quote is not quoted" true "$h_quote" 'android/*'
answer "a path with a newline that looks like a workflow command is matched" true \
    "$h_forged" 'note*'
unannotated "a path with a newline cannot make a workflow command"

echo
echo "paths with characters that mean something in a pattern"

answer "a * in a path is matched by the same * in a pattern" true "$h_star" 'a*'
answer "a * in a path is matched by a bracket expression" true "$h_star" 'a[*]'
answer "a * in a path is matched by an escaped *" true "$h_star" 'a\*'
answer "a path with a * is not matched by a pattern for another name" false "$h_star" 'b*'
answer "a path with a * does not act as a pattern" false "$h_star" 'ab'
answer "a [ in a path is matched by a bracket expression" true "$h_bracket" 'b[[]1]'
answer "a [ in a path is matched by a * in a pattern" true "$h_bracket" 'b*'
answer "a path with brackets is not matched by the pattern it looks like" false "$h_bracket" 'b[1]'
answer "a path with brackets does not act as a pattern" false "$h_bracket" 'b1'
answer "a ? in a path is matched by the same ? in a pattern" true "$h_question" 'c?'
answer "a ? in a path is matched by a bracket expression" true "$h_question" 'c[?]'
answer "a path with a ? is not matched by a pattern for a longer name" false "$h_question" 'c??'
answer "a path with a ? does not act as a pattern" false "$h_question" 'cd'
answer "a backslash in a path is matched by an escaped backslash" true "$h_backslash" 'd\\e'
answer "a backslash in a path is matched by a * in a pattern" true "$h_backslash" 'd*'
answer "a path with a backslash is not matched by the pattern it looks like" false \
    "$h_backslash" 'd\e'
answer "a path with a backslash does not act as a pattern" false "$h_backslash" 'de'
answer "a path that starts with a dash is matched by its own name" true "$h_dash" '--force'
answer "a path that starts with a dash is matched by a * in a pattern" true "$h_dash" '-*'
answer "a path that starts with a dash is not matched without its dashes" false "$h_dash" 'force'

echo
echo "the range is what the pull request changes"

answer "what the base branch gained is not a change of the pull request" false "$h_readme" \
    'android/*' PR_BASE_SHA="$main_moved"
answer "what the pull request changes counts when the base branch has moved on" true \
    "$h_android" 'android/*' PR_BASE_SHA="$main_moved"

echo
echo "the answer is true when the changes cannot be listed"

answer "a missing base commit" true "$h_readme" 'android/*' PR_BASE_SHA=
logged "a missing base commit is logged" "missing"
warned "a missing base commit is a warning"
answer "a missing head commit" true "$h_readme" 'android/*' PR_HEAD_SHA=
logged "a missing head commit is logged" "missing"
warned "a missing head commit is a warning"
answer "a base and a head commit that are both missing" true "$h_readme" 'android/*' \
    PR_BASE_SHA= PR_HEAD_SHA=
warned "two missing commits are a warning"
answer "a base commit that no repository holds" true "$h_readme" 'android/*' \
    PR_BASE_SHA="$UNKNOWN_SHA"
logged "an unknown base commit is logged" "could not list"
warned "an unknown base commit is a warning"
answer "a head commit that no repository holds" true "$h_readme" 'android/*' \
    PR_HEAD_SHA="$UNKNOWN_SHA"
logged "an unknown head commit is logged" "could not list"
warned "an unknown head commit is a warning"
answer "a base commit that is not a hash" true "$h_readme" 'android/*' PR_BASE_SHA=not-a-commit
warned "a base commit that is not a hash is a warning"
answer "a temporary directory that does not exist" true "$h_readme" 'android/*' \
    TMPDIR="$WORK/does-not-exist"
logged "a temporary file that cannot be made is logged" "temporary file"
warned "a temporary file that cannot be made is a warning"
in_dir "$WORK/home" answer "a directory that is not a repository" true "$h_readme" 'android/*'
logged "a directory that is not a repository is logged" "could not list"
warned "a directory that is not a repository is a warning"
in_dir "$WORK/shallow" answer "a shallow clone without the base commit" true "$h_readme" 'android/*'
logged "a shallow clone is logged" "could not list"
warned "a shallow clone is a warning"
answer "base and head commits with no history in common" true "$h_unrelated" 'android/*'
logged "base and head commits with no history in common are logged" "could not list"
warned "base and head commits with no history in common are a warning"

echo
echo "events other than pull_request"

answer "a push is answered true without looking at the changes" true "$h_readme" 'android/*' \
    EVENT_NAME=push
logged "a push says why everything runs" "not a pull request"
unannotated "a push is a plain line of the log"
answer "a push does not look at the commits" true "$h_readme" 'android/*' EVENT_NAME=push \
    PR_BASE_SHA="$UNKNOWN_SHA"
unlogged "a push does not try to list the changes" "could not list"
answer "a manual run is answered true" true "$h_readme" 'android/*' EVENT_NAME=workflow_dispatch \
    PR_BASE_SHA= PR_HEAD_SHA=
unlogged "a manual run does not complain about the missing commits" "missing"
answer "any other event is answered true" true "$h_readme" 'android/*' EVENT_NAME=schedule
refuses "an event without a name is an error" "name of the event is missing" "$h_readme" \
    'android/*' EVENT_NAME=
in_dir "$WORK/home" answer "an event that is not a pull request does not need a repository" true \
    "$h_readme" 'android/*' EVENT_NAME=push

echo
echo "the patterns are checked on every event"

refuses "a push with no pattern is an error" "no patterns" "$h_readme" '' EVENT_NAME=push
refuses "a manual run with no pattern is an error" "no patterns" "$h_readme" '' \
    EVENT_NAME=workflow_dispatch PR_BASE_SHA= PR_HEAD_SHA=
refuses "an event without a name and with no pattern is an error" "no patterns" "$h_readme" '' \
    EVENT_NAME=
refuses "only comments and blank lines is an error on a push" "no patterns" "$h_readme" \
    $'\n# android/*\n   \n' EVENT_NAME=push
in_dir "$WORK/home" refuses "no pattern is an error outside a repository, too" "no patterns" \
    "$h_readme" '' EVENT_NAME=push

echo
echo "the temporary file"

mkdir "$WORK/tmp"
answer "a pull request that matches" true "$h_android" 'android/*' TMPDIR="$WORK/tmp"
answer "a pull request that does not match" false "$h_readme" 'android/*' TMPDIR="$WORK/tmp"
answer "a pull request whose changes cannot be listed" true "$h_readme" 'android/*' \
    TMPDIR="$WORK/tmp" PR_BASE_SHA="$UNKNOWN_SHA"
if [[ -z "$(ls -A "$WORK/tmp")" ]]; then
    pass "no temporary file is left behind"
else
    fail "no temporary file is left behind" "$(ls -A "$WORK/tmp")"
fi

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
