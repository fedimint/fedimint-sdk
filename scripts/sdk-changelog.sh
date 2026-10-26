#!/usr/bin/env bash
#
# Reads and writes the release notes of the SDK's components, which each keep
# them in a CHANGELOG.md of their own.
#
#   scripts/sdk-changelog.sh section <component> <version>
#   scripts/sdk-changelog.sh release-notes <component> <version>
#   scripts/sdk-changelog.sh check <component> <version>
#   scripts/sdk-changelog.sh write <component> <version> [<previous version>]
#   scripts/sdk-changelog.sh left-out <component> <previous version>
#   scripts/sdk-changelog.sh commits --tag-prefix <prefix> --since <version> [--to <commit>] <path>...
#
# The components are defined in scripts/sdk-components.sh. Each release of one
# has a `## <version>` section in its file, newest first:
#
#   rust          rust/fedimint-sdk/CHANGELOG.md
#   android       android/CHANGELOG.md
#   react-native  js/react-native/CHANGELOG.md
#
# `section` prints a version's notes as the file has them. `release-notes`
# prints them for the version's GitHub Release, which shows each line break of
# its text: every paragraph and every list item is joined onto one line,
# however the file wraps it. A blank line, a heading line and the start of a
# list item (`-`, `*`, `+` or a number and a dot) each start a new line, and a
# list item keeps its indentation. A heading line is one or more `#` followed
# by whitespace or by the end of the line, so a wrapped line that starts with
# an issue reference such as `#447` is part of its paragraph. A fence line is a
# line that starts with three backticks after any indentation, so a fenced code
# block can sit inside a list item. The fence lines and the lines between them
# are printed exactly as they are, indentation included. Nothing else is told
# apart from a paragraph, so a block quote, a table or an indented code block
# would be joined onto one line: use a list or a fenced block.
#
# `check` fails unless the section exists and has something in it. The release
# refuses a version that fails it, so nothing is published without notes.
#
# `write` adds a section for <version> above the others, or at the end of the
# file when it has no section yet. It is written from the commits since the
# component's release tag for <previous version> that touched what the
# component is built from, the ones `commits` lists, newest first. Their
# subjects are read as conventional commits, `<type>(<scope>)!: <summary>` with
# the scope and the `!` optional, and a commit goes under the heading its
# subject calls for:
#
#   ### Breaking   any type, with a `!` before the colon
#   ### Added      `feat`
#   ### Fixed      `fix`
#
# A fourth heading, `### Changed`, holds the one line that is not a commit,
# about the fedimint the crate is built against, as described below. The
# section has the headings that have entries, in the order Breaking, Added,
# Changed, Fixed. An entry of the other three is a list item holding the
# subject without its `<type>(<scope>)!: ` prefix, as it is otherwise, so that
# the number of a pull request, which a squash merge appends to the subject,
# stays. A commit of any other type, such as the `chore(release): ` commit of
# a version bump, is left out, and so is a commit whose subject is not in that
# form.
#
# A commit can touch what the component is built from and still not be about
# it, such as a change of the development environment that edits flake.nix.
# Its scope says so. A commit whose scope is one of the component's foreign
# scopes, which `component_foreign_scopes` in sdk-components.sh lists with what
# makes a scope foreign, is left out whatever its type, a `!` included. The
# scope is compared whole, so `sdk-web` is not `web`. A commit with no scope,
# or with any other scope, is read as above, so a commit with a scope that is
# not listed is listed rather than lost.
#
# A revert, `Revert "<subject>"` with ` (#<number>)` optionally after it, is
# never an entry. It takes away one entry, the one of the commit in the same
# range whose subject is <subject>, or <subject> followed by ` (#<number>)`.
# `git revert` and GitHub's Revert button write the subject of the reverted
# commit, number included, but GitHub titles the revert's pull request
# `Revert "<title>"`, without it, and a pull request of several commits lands
# with its title.
# `git log` lists the newest commit first, so a revert is read before the
# commit it reverts. A revert that was itself reverted takes away nothing, so
# the entry of the commit stays. A later `Revert "Revert "<subject>""` reverts
# it, and so does a later `Reapply "<subject>"`, which git 2.43 and later
# write for the revert of a revert and which is never an entry either.
# Followed by any number of ` (#<number>)`, it names the revert
# `Revert "<subject>"` followed by the same numbers, or by all but the last,
# which a squash merge may have added to the reapply's own pull request. A
# revert of a commit from an earlier release takes away nothing and is left
# out: write a line about it by hand in the version bump pull request.
#
# The crate's lockfile, rust/fedimint-sdk/Cargo.lock, says which fedimint the
# component is built against. `write` reads its first `fedimint-core` package
# at the release tag and at HEAD, and calls each `fedimint <version>` when its
# source is a registry, or `an unreleased fedimint` when it is anything else,
# a git revision. When the two differ in version or source, so that two git
# revisions differ, the section gets the line `Built against <new>, where
# <previous version> was built against <old>.` under `### Changed`, or `Built
# against another unreleased fedimint than <previous version>.` when both are
# unreleased. A lockfile or package that is missing at either commit gives no
# line and no error, and neither does a first release. The line is not a
# commit, so the stderr line below does not count it.
#
# A section that has no entry says `No changes for users since <previous
# version>.`, and the section of a component that was never released, which is
# written without <previous version>, says `The first release of the <title>.`,
# with the title that sdk-components.sh gives the component. Nothing goes to
# stdout. One line on stderr gives the number of commits it looked at and how
# many of them are in the notes. The bump workflow runs it for the version bump
# pull request. It needs the git history back to that tag.
#
# `left-out` prints the commits `write` reads for the component since
# <previous version> and gives no entry, newest first, as `commits` does: a
# commit of a type that is not listed, a subject that is not in the form, a
# commit with a foreign scope, a revert or a reapply, and a commit that a
# revert took away. It prints nothing when every commit has an entry, reads no
# file, and fails when the tag is not in the checkout, as `commits` does. The
# bump workflow lists the commits in the version bump pull request, for whoever
# reads the notes to add a line about what users see among them. `write` and
# `left-out` sort the commits with one function, so they cannot disagree.
#
# `commits` prints the commits `write` selects, for any SDK: the non-merge
# commits from the tag `<prefix><version>` up to <commit> (HEAD when not given)
# that touched at least one of the paths, newest first, as
# `- <subject> (<abbreviated hash>)`. It reads no file, so a release can write
# its notes from it without a CHANGELOG.md. It fails when the tag, annotated or
# lightweight, is not in the checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/sdk-components.sh
. "$ROOT/scripts/sdk-components.sh"
# The commit subjects `write` lists, by the heading they go under: any type
# with a `!` before the colon is Breaking, `feat` is Added and `fix` is Fixed.
# Each pattern ends with the space after the colon, so what it matches is the
# prefix to remove, and its second group is the scope, which is empty when the
# subject has none.
BREAKING_SUBJECT='^[a-z]+(\(([^)]*)\))?!: '
ADDED_SUBJECT='^feat(\(([^)]*)\))?: '
FIXED_SUBJECT='^fix(\(([^)]*)\))?: '
# A revert as git and GitHub write it, followed by the number of its pull
# request when a squash merge added one. Its first group is the subject it
# reverts.
REVERT_SUBJECT='^Revert "(.+)"( \(#[0-9]+\))?$'
# A reapply, which git 2.43 and later write for the revert of a revert, followed
# by any number of pull request numbers. Its first group is the subject of the
# revert it undoes, and its second the numbers with their parentheses.
REAPPLY_SUBJECT='^Reapply "(.+)"(( \(#[0-9]+\))*)$'
# A subject that has a pull request number at its end. Its first group is the
# subject without it.
NUMBERED_SUBJECT='^(.+) \(#[0-9]+\)$'
# What the notes call a fedimint that is not a release.
UNRELEASED_FEDIMINT="an unreleased fedimint"
# The component the command is about, and where its notes are: the file
# relative to the repository, and the path to it. select_component sets them.
COMPONENT=""
FILE=""
CHANGELOG=""
# The commits `commits`, `write` and `left-out` take: the range of git
# revisions they are in, its two ends, and the paths that each touched.
# select_commits sets them.
RANGE=""
RANGE_FROM=""
RANGE_TO=""
RANGE_PATHS=()
# What classify_commits finds: the list items of the Breaking, Added and Fixed
# groups and the lines of the commits that have no entry, each ending with a
# newline, and the number of commits it looked at and put into the notes.
ENTRIES_BREAKING=""
ENTRIES_ADDED=""
ENTRIES_FIXED=""
LEFT_OUT=""
LOOKED=0
LISTED=0

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Selects the component $1, which must be one, and with it the notes that
# `section`, `release-notes`, `check` and `write` work on. `left-out` selects it
# the same way, but never opens its notes.
select_component() {
    is_component "$1" || exit 1
    COMPONENT="$1"
    FILE="$(component_changelog "$COMPONENT")" || exit 1
    CHANGELOG="$ROOT/$FILE"
}

# Fails unless the selected component's notes exist.
need_changelog() {
    [[ -f "$CHANGELOG" ]] || die "no $CHANGELOG"
}

# Prints the lines under `## $1` up to the next `## ` heading, without the
# blank lines around them. Fails when there is no such section.
section() {
    local version="$1"
    grep -qxF "## $version" "$CHANGELOG" || return 1
    awk -v heading="## $version" '
        $0 == heading { inside = 1; next }
        inside && /^## / { exit }
        inside { lines[++n] = $0 }
        END {
            first = 1; while (first <= n && lines[first] ~ /^[[:space:]]*$/) first++
            last = n;  while (last >= first && lines[last] ~ /^[[:space:]]*$/) last--
            for (i = first; i <= last; i++) print lines[i]
        }
    ' "$CHANGELOG"
}

# Prints the lines of `section $1` with each paragraph and each list item on a
# single line. A blank line, a heading line (one or more `#` followed by
# whitespace or by the end of the line), the start of a list item and a fence
# line (three backticks after any indentation) each start a new output line.
# Any other line continues the one before it, without its leading whitespace
# and with a single space between them. A heading line is a line of its own, so
# the line after it starts a new one. The fence lines and the lines between
# them are printed as they are. Trailing whitespace is dropped from every other
# line, so that a join always has a single space. Fails when there is no such
# section.
release_notes() {
    section "$1" | awk '
        function flush() {
            if (pending) { print out; pending = 0 }
        }
        fenced { print; if ($0 ~ /^[[:space:]]*```/) fenced = 0; next }
        /^[[:space:]]*```/ { flush(); print; fenced = 1; next }
        {
            line = $0
            sub(/[[:space:]]+$/, "", line)
        }
        line == "" { flush(); print ""; next }
        line ~ /^#+$/ || line ~ /^#+[[:space:]]/ { flush(); print line; next }
        line ~ /^[[:space:]]*([-*+]|[0-9]+\.)[[:space:]]/ { flush(); out = line; pending = 1; next }
        pending { sub(/^[[:space:]]+/, "", line); out = out " " line; next }
        { out = line; pending = 1 }
        END { flush() }
    '
}

# select_commits --tag-prefix <prefix> --since <version> [--to <commit>] <path>...
# Takes the commits in `<prefix><version>..<commit>` (<commit> is HEAD when not
# given) that touched one of the paths, by setting RANGE to the revisions,
# RANGE_FROM and RANGE_TO to their two ends and RANGE_PATHS to the paths. Fails
# when the tag or the commit is not in the checkout, and prints the usage when
# an option has no value or one is missing.
select_commits() {
    local prefix="" since="" to=HEAD tag target
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --tag-prefix | --since | --to)
                [[ $# -ge 2 ]] || usage
                case "$1" in
                    --tag-prefix) prefix="$2" ;;
                    --since) since="$2" ;;
                    --to) to="$2" ;;
                esac
                shift 2
                ;;
            -*) usage ;;
            *) break ;;
        esac
    done
    [[ -n "$prefix" && -n "$since" && $# -gt 0 ]] || usage
    tag="refs/tags/$prefix$since"
    git -C "$ROOT" rev-parse -q --verify "$tag" >/dev/null ||
        die "no tag $prefix$since in this checkout; fetch the tags and history"
    target="$(git -C "$ROOT" rev-parse -q --verify "$to^{commit}")" ||
        die "no commit $to in this checkout"
    RANGE="$tag..$target"
    RANGE_FROM="$tag"
    RANGE_TO="$target"
    RANGE_PATHS=("$@")
}

# commits --tag-prefix <prefix> --since <version> [--to <commit>] <path>...
# Prints "- <subject> (<hash>)" for each non-merge commit in
# `<prefix><version>..<commit>` that touched one of the paths, newest first.
commits() {
    select_commits "$@"
    git -C "$ROOT" log --no-merges --format='- %s (%h)' "$RANGE" -- "${RANGE_PATHS[@]}"
}

check() {
    local version="$1" notes
    notes="$(section "$version")" ||
        die "$FILE has no \`## $version\` section; add the notes for $version"
    [[ -n "$notes" ]] || die "$FILE's \`## $version\` section is empty"
    echo "$FILE has notes for $version"
}

# Prints the heading $1 and the list items $2, each of which ends with a newline,
# followed by a blank line, or nothing when there are no items.
notes_group() {
    [[ -z "$2" ]] || printf '### %s\n\n%s\n' "$1" "$2"
}

# Prints the `<version> <source>` of the first `fedimint-core` package in the
# lockfile of the crate at the revision $1, as its `[[package]]` block has them
# (the source is empty when the block has none). Prints nothing when the
# lockfile, the package or its version is missing at that revision.
fedimint_core_at() {
    local lock
    lock="$(git -C "$ROOT" show "$1:rust/fedimint-sdk/Cargo.lock" 2>/dev/null)" || return 0
    awk '
        /^\[/ {
            if (name == "fedimint-core") exit
            packages = ($0 == "[[package]]")
            name = ""; version = ""; source = ""
            next
        }
        packages && /^(name|version|source) = "/ {
            value = $0
            sub(/^[a-z]+ = "/, "", value)
            sub(/"$/, "", value)
            if ($1 == "name") name = value
            else if ($1 == "version") version = value
            else source = value
        }
        END { if (name == "fedimint-core" && version != "") print version " " source }
    ' <<<"$lock"
}

# Describes for the notes the `<version> <source>` $1 of a fedimint-core
# package: a release when its source is a registry, any other is not.
fedimint_text() {
    if [[ "${1#* }" == registry+* ]]; then
        echo "fedimint ${1%% *}"
    else
        echo "$UNRELEASED_FEDIMINT"
    fi
}

# Prints the entry of the `### Changed` group when the crate is built against
# another fedimint at the end of the selected range than at its start, where
# the release $1 is the start, and nothing otherwise. See the header.
fedimint_line() {
    local previous="$1" old new old_text new_text
    old="$(fedimint_core_at "$RANGE_FROM")"
    new="$(fedimint_core_at "$RANGE_TO")"
    [[ -n "$old" && -n "$new" && "$old" != "$new" ]] || return 0
    old_text="$(fedimint_text "$old")"
    new_text="$(fedimint_text "$new")"
    if [[ "$old_text" == "$UNRELEASED_FEDIMINT" && "$new_text" == "$UNRELEASED_FEDIMINT" ]]; then
        echo "- Built against another unreleased fedimint than $previous."
    elif [[ "$old_text" != "$new_text" ]]; then
        echo "- Built against $new_text, where $previous was built against $old_text."
    fi
}

# Sorts the commits since the release $1 of the selected component, newest
# first, into the notes and out of them, as the header describes. It sets
# ENTRIES_BREAKING, ENTRIES_ADDED, ENTRIES_FIXED, LEFT_OUT, LOOKED and LISTED,
# for `write` and `left-out` to share.
classify_commits() {
    local previous="$1" prefix path log line hash subject base group scope item quoted numbers
    local foreign reverted nl=$'\n'
    local -a paths=()
    ENTRIES_BREAKING="" ENTRIES_ADDED="" ENTRIES_FIXED="" LEFT_OUT="" LOOKED=0 LISTED=0
    prefix="$(component_tag_prefix "$COMPONENT")" || exit 1
    # `git log` lists the newest commit first, so a revert is read before
    # the commit it reverts. `reverted` holds the subjects that the
    # reverts read so far name, and `foreign` the scopes that are not
    # about the component. Each has one item per line and starts and ends
    # with a newline, so that `*"$nl$item$nl"*` finds a whole item and
    # nothing else.
    foreign="$(component_foreign_scopes "$COMPONENT")" || exit 1
    foreign="$nl$foreign$nl"
    reverted="$nl"
    while IFS= read -r path; do
        paths+=("$path")
    done < <(component_paths "$COMPONENT")
    select_commits --tag-prefix "$prefix" --since "$previous" "${paths[@]}"
    log="$(git -C "$ROOT" log --no-merges --format='%h %s' "$RANGE" -- "${RANGE_PATHS[@]}")"
    # Without a commit there is no line to read, but a here-string would
    # still give the loop one empty line.
    [[ -n "$log" ]] || return 0
    while IFS= read -r line; do
        hash="${line%% *}"
        subject="${line#* }"
        LOOKED=$((LOOKED + 1))
        group=""
        # The subject without the pull request number that a squash merge
        # added, which the title of a revert pull request does not have.
        base="$subject"
        if [[ "$subject" =~ $NUMBERED_SUBJECT ]]; then
            base="${BASH_REMATCH[1]}"
        fi
        # A commit that a revert named is not an entry, whether the revert
        # wrote its number or not. Neither is a revert that a newer revert or
        # reapply named, which takes away nothing. Each name is used up by one
        # commit.
        if [[ "$reverted" == *"$nl$subject$nl"* ]]; then
            reverted="${reverted/"$nl$subject$nl"/$nl}"
        elif [[ "$reverted" == *"$nl$base$nl"* ]]; then
            reverted="${reverted/"$nl$base$nl"/$nl}"
        elif [[ "$subject" =~ $REAPPLY_SUBJECT ]]; then
            # It undoes the revert that has its subject and its numbers but
            # the last, which a squash merge may have added. A revert that
            # has all of them is found without its last number, as above.
            quoted="${BASH_REMATCH[1]}"
            numbers="${BASH_REMATCH[2]}"
            reverted+="Revert \"$quoted\"${numbers% (#*)}$nl"
        elif [[ "$subject" =~ $REVERT_SUBJECT ]]; then
            reverted+="${BASH_REMATCH[1]}$nl"
        elif [[ "$subject" =~ $BREAKING_SUBJECT ]]; then
            group=breaking
        elif [[ "$subject" =~ $ADDED_SUBJECT ]]; then
            group=added
        elif [[ "$subject" =~ $FIXED_SUBJECT ]]; then
            group=fixed
        fi
        if [[ -n "$group" ]]; then
            scope="${BASH_REMATCH[2]}"
            item="- ${subject#"${BASH_REMATCH[0]}"}"$nl
            if [[ -n "$scope" && "$foreign" == *"$nl$scope$nl"* ]]; then
                group=""
            fi
        fi
        if [[ -z "$group" ]]; then
            LEFT_OUT+="- $subject ($hash)$nl"
            continue
        fi
        case "$group" in
            breaking) ENTRIES_BREAKING+="$item" ;;
            added) ENTRIES_ADDED+="$item" ;;
            fixed) ENTRIES_FIXED+="$item" ;;
        esac
        LISTED=$((LISTED + 1))
    done <<<"$log"
}

# left-out <previous version>
# Prints the commits since the release $1 of the selected component that its
# notes have no entry for, as `commits` prints them. See the header.
left_out() {
    classify_commits "$1"
    printf '%s' "$LEFT_OUT"
}

# Adds the section for the version $1 to the notes of the selected component,
# written from the commits since the release $2, or as the section of a first
# release when $2 is empty. See the header.
write() {
    local version="$1" previous="${2:-}" entry summary tmp changed=""
    if grep -qxF "## $version" "$CHANGELOG"; then
        die "$FILE already has a \`## $version\` section"
    fi
    if [[ -n "$previous" ]]; then
        classify_commits "$previous"
        changed="$(fedimint_line "$previous")"
        [[ -z "$changed" ]] || changed+=$'\n'
        entry="$(
            notes_group Breaking "$ENTRIES_BREAKING"
            notes_group Added "$ENTRIES_ADDED"
            notes_group Changed "$changed"
            notes_group Fixed "$ENTRIES_FIXED"
        )"
        [[ -n "$entry" ]] || entry="No changes for users since $previous."
        summary="$FILE has notes for $version from $LISTED of $LOOKED commits since $previous"
    else
        entry="The first release of the $(component_title "$COMPONENT")."
        summary="$FILE has notes for $version, those of a first release"
    fi
    # The new section goes above the newest one, or at the end if there is none.
    # The entry goes through the environment rather than `awk -v`, which would
    # read backslashes in commit subjects as escapes and, in BSD awk, refuses
    # a value with newlines.
    tmp="$(mktemp)"
    ENTRY="$entry" awk -v heading="## $version" '
        BEGIN { entry = ENVIRON["ENTRY"] }
        !done && /^## / { print heading; print ""; print entry; print ""; done = 1 }
        { print }
        END { if (!done) { print ""; print heading; print ""; print entry } }
    ' "$CHANGELOG" >"$tmp"
    mv "$tmp" "$CHANGELOG"
    echo "$summary" >&2
}

case "${1:-}" in
    section)
        [[ $# -eq 3 ]] || usage
        select_component "$2"
        need_changelog
        section "$3" || die "$FILE has no \`## $3\` section"
        ;;
    release-notes)
        [[ $# -eq 3 ]] || usage
        select_component "$2"
        need_changelog
        release_notes "$3" || die "$FILE has no \`## $3\` section"
        ;;
    check)
        [[ $# -eq 3 ]] || usage
        select_component "$2"
        need_changelog
        check "$3"
        ;;
    write)
        [[ $# -eq 3 || $# -eq 4 ]] || usage
        select_component "$2"
        need_changelog
        write "$3" "${4:-}"
        ;;
    left-out)
        [[ $# -eq 3 ]] || usage
        select_component "$2"
        left_out "$3"
        ;;
    commits)
        shift
        commits "$@"
        ;;
    *) usage ;;
esac
