#!/usr/bin/env bash
#
# Measures the browser SDK's wasm module, reports how a pull request changed its size, and checks
# that the pull request does not make it grow too much.
#
#   scripts/sdk-web-wasm-size.sh measure <module>
#   scripts/sdk-web-wasm-size.sh report <head sizes file> [<base sizes file>]
#   scripts/sdk-web-wasm-size.sh check <head sizes file> [<base sizes file>]
#
# `measure` prints one line, `<bytes> <gzip bytes>`: the size of the file and its size after
# `gzip -9`, which is about what a browser downloads from a server that compresses. The module to
# measure is the one js/web/sdk-web ships, js/web/sdk-web/src/generated/fedimint_sdk.wasm, as
# scripts/generate-sdk-web-bindings.sh leaves it, optimised for size. The compressed size comes
# from the `gzip` on PATH, and another gzip can compress a little differently, so only compare
# sizes that came from the same gzip. Even then the compressed size moves by a hundred bytes or
# so between two runs of the generator on the same crate, because wasm-bindgen does not write the
# same bytes on every run.
#
# `report` reads two files that each hold such a line, the pull request's (the head) and the
# base's, and prints a Markdown section for a GitHub job summary: a table with the size of the
# module and of its compressed form for both, and the difference between them in bytes and in
# percent of the base. A size is shown in MB (1000000 bytes) with two decimals and, beside it,
# in bytes; the difference in bytes is exact, as even a few kilobytes are worth seeing in a
# module of more than ten megabytes. The percentage has one decimal and a sign, and a half rounds
# away from zero. A change too small to show keeps its sign (`-0.0%`), and no change is `+0.0%`,
# whatever the base. A change from a base of 0 bytes has no percentage, and the table shows `n/a`
# for it. The text under the table says what the two columns are, which differences mean nothing
# and how much growth `check` allows. It describes the `Check Wasm Size` job of
# .github/workflows/verify.yml, which writes the report and runs `check`, where the base is the
# commit that the pull request is merged into and both modules are generated with the pull
# request's tools.
#
# `check` reads the same two files and fails when the head is too big: when its size, or its size
# after gzip -9, exceeds the base's by more than MAX_GROWTH_PERCENT percent of the base's. It then
# prints a line for each size that grew too much, with the size of the base and of the head, the
# growth and the most that a pull request may add, and exits with status 1. Otherwise it prints
# nothing.
#
# Without a base file, or when that file is missing, empty or not a sizes line, the report holds
# the pull request's sizes only and says there is no size for the base to compare with, and
# `check` prints nothing and passes. The head file must be a sizes line: if it is not, both
# commands fail with an error on stderr and print nothing on stdout.
#
# Anything else prints the usage and exits with status 2.
set -euo pipefail

# A sizes line is two decimal numbers separated by a space. They have no leading zero, which
# bash arithmetic would read as octal, and at most 15 digits, so that the arithmetic below cannot
# overflow.
SIZES_RE='^(0|[1-9][0-9]{0,14}) (0|[1-9][0-9]{0,14})$'

# How much a pull request may make the module grow, as a percentage of the base's size. It holds
# for the module as it is and after gzip -9 alike, and `check` fails on more. The README of
# js/web/sdk-web and the `wasm-size` job of .github/workflows/verify.yml give the value too.
MAX_GROWTH_PERCENT=5

die() {
    echo "$*" >&2
    exit 1
}

usage() {
    sed -n '/^#   scripts/p' "${BASH_SOURCE[0]}" | sed 's/^#   /usage: /' >&2
    exit 2
}

# Prints the sizes line in the file $1, and fails when there is no such file or it holds anything
# else. The newline that ends the line is not part of it.
sizes_line() {
    local line
    [[ -f "$1" ]] || return 1
    line="$(<"$1")"
    [[ "$line" =~ $SIZES_RE ]] || return 1
    echo "$line"
}

# Prints the sizes line in the head's sizes file $1, and exits with an error that names the file
# when there is no such file or it holds anything else.
head_sizes_line() {
    [[ -f "$1" ]] || die "no sizes file at $1"
    sizes_line "$1" || die "$1 is not a sizes file; it must hold one line, <bytes> <gzip bytes>"
}

# Prints the size $1 in bytes as megabytes, rounded half up, and as the exact bytes.
size() {
    local hundredths=$(( ($1 + 5000) / 10000 ))
    printf '%d.%02d MB (%d B)\n' $((hundredths / 100)) $((hundredths % 100)) "$1"
}

# Prints the change from the size $1 to the size $2 in bytes, with its sign.
change() {
    local diff=$(($2 - $1))
    if ((diff < 0)); then
        echo "$diff"
    else
        echo "+$diff"
    fi
}

# Prints the change from the size $1 to the size $2 in percent of $1, with one decimal and a sign.
# It is worked out in whole tenths of a percent, so the rounding is exact. Nothing is a percentage
# of 0, so a change from 0 is n/a, but no change is +0.0% there as everywhere.
percent() {
    local base="$1" sign="+" diff tenths
    diff=$(($2 - base))
    if ((diff < 0)); then
        sign="-"
        diff=$((-diff))
    fi
    if ((base == 0)); then
        if ((diff == 0)); then echo "+0.0%"; else echo "n/a"; fi
        return
    fi
    tenths=$(( (diff * 2000 + base) / (2 * base) ))
    printf '%s%d.%d%%\n' "$sign" $((tenths / 10)) $((tenths % 10))
}

# Prints the table row $1 for the base's size $2 in bytes and the head's size $3.
compared_row() {
    printf '| %s | %s | %s | %s | %s |\n' "$1" "$(size "$2")" "$(size "$3")" \
        "$(change "$2" "$3")" "$(percent "$2" "$3")"
}

# Prints the table row $1 for the head's size $2 in bytes.
head_row() {
    printf '| %s | %s |\n' "$1" "$(size "$2")"
}

# Prints "<bytes> <gzip bytes>" for the module $1. The module goes in through stdin: gzip would
# put the file's name in the header of what it writes, and the name is no part of the size.
measure() {
    local bytes gzipped
    [[ -f "$1" ]] || die "no module at $1"
    bytes="$(wc -c <"$1" | tr -d ' ')"
    gzipped="$(gzip -9 -c <"$1" | wc -c | tr -d ' ')"
    echo "$bytes $gzipped"
}

# Prints the report for the sizes files $1 (the head's) and $2 (the base's, which may be "").
report() {
    local head_sizes base_sizes
    head_sizes="$(head_sizes_line "$1")" || exit 1
    base_sizes="$(sizes_line "$2")" || base_sizes=""

    cat <<'EOF'
### Size of the browser SDK's wasm module

The size of `fedimint_sdk.wasm`, the module that `@fedimint/sdk-web` ships, as it is and after
`gzip -9`.

EOF
    if [[ -n "$base_sizes" ]]; then
        echo '| | Base | Pull request | Difference (bytes) | Difference (% of base) |'
        echo '| --- | ---: | ---: | ---: | ---: |'
        compared_row "Module" "${base_sizes% *}" "${head_sizes% *}"
        compared_row "Module, gzip -9" "${base_sizes#* }" "${head_sizes#* }"
        cat <<'EOF'

"Base" is the commit of the base branch that the pull request is merged into in the checkout of
this run, and "Pull request" is the result of that merge. Both modules went through the pull
request's `scripts/generate-sdk-web-bindings.sh` and its tools, so a change to that script or to
those tools does not show in the difference.

A difference of a hundred bytes or so in the gzip row is not a change: the generator does not
write the same bytes on every run, even from the same crate.
EOF
        echo
        echo "The check fails when the pull request makes either size grow by more than" \
            "${MAX_GROWTH_PERCENT}% of the base's."
    else
        echo '| | Pull request |'
        echo '| --- | ---: |'
        head_row "Module" "${head_sizes% *}"
        head_row "Module, gzip -9" "${head_sizes#* }"
        cat <<'EOF'

There is no size for the base commit to compare with, so the growth of the module is not
checked. The log of the `Test` job says why.
EOF
    fi
}

# Checks that the size $3 in bytes grew from the size $2 by at most MAX_GROWTH_PERCENT percent of
# $2. When it grew by more, prints a line that starts with the name $1 of the size and says by how
# much it grew and by how much it may grow, and fails. Otherwise prints nothing. The test is made
# in whole bytes and without a division: a growth is more than the allowance, which is rounded
# down, exactly when the growth times 100 is more than $2 times MAX_GROWTH_PERCENT.
check_growth() {
    local name="$1" base="$2" head="$3"
    (( (head - base) * 100 > base * MAX_GROWTH_PERCENT )) || return 0
    echo "$name grew by $((head - base)) B, from $(size "$base") to $(size "$head")," \
        "and a pull request may add at most ${MAX_GROWTH_PERCENT}% of the base," \
        "$((base * MAX_GROWTH_PERCENT / 100)) B."
    return 1
}

# Prints a line for each size in the head's sizes file $1 that grew too much from the size in the
# base's sizes file $2 (which may be "") and fails if there is one. Without a base there is nothing
# to compare with, so it prints nothing and succeeds.
check() {
    local head_sizes base_sizes failed=0
    head_sizes="$(head_sizes_line "$1")" || exit 1
    base_sizes="$(sizes_line "$2")" || return 0
    check_growth "The module" "${base_sizes% *}" "${head_sizes% *}" || failed=1
    check_growth "The module after gzip -9" "${base_sizes#* }" "${head_sizes#* }" || failed=1
    return "$failed"
}

case "${1:-}" in
    measure)
        if (($# != 2)); then usage; fi
        measure "$2"
        ;;
    report)
        if (($# != 2 && $# != 3)); then usage; fi
        report "$2" "${3:-}"
        ;;
    check)
        if (($# != 2 && $# != 3)); then usage; fi
        check "$2" "${3:-}"
        ;;
    *)
        usage
        ;;
esac
