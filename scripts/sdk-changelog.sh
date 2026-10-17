#!/usr/bin/env bash
#
# Reads and drafts the release notes of the SDK's components, which each keep
# them in a CHANGELOG.md of their own.
#
#   scripts/sdk-changelog.sh section <component> <version>
#   scripts/sdk-changelog.sh release-notes <component> <version>
#   scripts/sdk-changelog.sh check <component> <version>
#   scripts/sdk-changelog.sh draft <component> <version> [<previous version>]
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
# `check` fails unless the section exists, has something in it, and is no
# longer a draft. The release refuses a version that fails it, so nothing is
# published without notes a person has read.
#
# `draft` adds a section for <version> above the others. It lists the commits
# since the component's release tag for <previous version> that touched what
# the component is built from, and it is marked as a draft. The commits that
# only bump a version are left out. The bump workflow runs it for the version
# bump pull request. The notes are rewritten for users in that pull request,
# and the marker is removed there. It needs the git history back to that tag.
#
# `commits` prints the commit list `draft` starts from, for any SDK: the
# non-merge commits from the tag `<prefix><version>` up to <commit> (HEAD when
# not given) that touched at least one of the paths, newest first, as
# `- <subject> (<abbreviated hash>)`. It reads no file, so a release can draft
# its notes from it without a CHANGELOG.md. It fails when the tag, annotated or
# lightweight, is not in the checkout.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/sdk-components.sh
. "$ROOT/scripts/sdk-components.sh"
DRAFT_MARKER='<!-- draft:'
# The commits that only bump a version and its notes, which a draft leaves out:
# those of the version bump workflow, whose subjects start with
# `chore(release): `. Matched after the `- ` each listed commit starts with, so
# no `^` here.
BUMP_SUBJECT='chore\(release\): '
# The component the command is about, and where its notes are: the file
# relative to the repository, and the path to it. select_component sets them.
COMPONENT=""
FILE=""
CHANGELOG=""

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Selects the component $1, which must be one, and with it the notes that
# `section`, `release-notes`, `check` and `draft` work on.
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

# commits --tag-prefix <prefix> --since <version> [--to <commit>] <path>...
# Prints "- <subject> (<hash>)" for each non-merge commit in
# `<prefix><version>..<commit>` that touched one of the paths, newest first.
commits() {
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
    git -C "$ROOT" log --no-merges --format='- %s (%h)' "$tag..$target" -- "$@"
}

check() {
    local version="$1" notes
    notes="$(section "$version")" ||
        die "$FILE has no \`## $version\` section; add the notes for $version"
    [[ -n "$notes" ]] || die "$FILE's \`## $version\` section is empty"
    if grep -qF "$DRAFT_MARKER" <<<"$notes"; then
        die "$FILE's \`## $version\` section is still a draft:" \
            "rewrite it and remove the draft comment"
    fi
    echo "$FILE has notes for $version"
}

draft() {
    local version="$1" previous="${2:-}" prefix path listed commits entry tmp
    local -a paths=()
    if grep -qxF "## $version" "$CHANGELOG"; then
        die "$FILE already has a \`## $version\` section"
    fi
    if [[ -n "$previous" ]]; then
        prefix="$(component_tag_prefix "$COMPONENT")" || exit 1
        while IFS= read -r path; do
            paths+=("$path")
        done < <(component_paths "$COMPONENT")
        listed="$(commits --tag-prefix "$prefix" --since "$previous" "${paths[@]}")"
        commits="$(grep -Ev -- "^- $BUMP_SUBJECT" <<<"$listed" || true)"
        [[ -n "$commits" ]] || commits="- No changes to the SDK since $previous."
        entry="$DRAFT_MARKER the commits since $previous that touched the SDK.
     Rewrite them for users: what was added, changed or fixed, and anything that
     breaks callers. Then delete this comment; the release refuses a draft. -->

$commits"
    else
        entry="$DRAFT_MARKER nothing is released yet, so there are no earlier
     commits to list. Write the notes for the first release, then delete this
     comment; the release refuses a draft. -->"
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
    draft)
        [[ $# -eq 3 || $# -eq 4 ]] || usage
        select_component "$2"
        need_changelog
        draft "$3" "${4:-}"
        ;;
    commits)
        shift
        commits "$@"
        ;;
    *) usage ;;
esac
