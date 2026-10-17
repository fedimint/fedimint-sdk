#!/usr/bin/env bash
#
# Tests scripts/sdk-web-wasm-size.sh, which measures the browser SDK's wasm module, reports its
# size and checks its growth on pull requests. .github/workflows/android-sdk-scripts.yaml runs
# this on pull requests that change either script.
#
#   scripts/test-sdk-web-wasm-size.sh
#
# Each case runs the real script on small files in a throwaway directory. Nothing in this
# checkout is touched, and nothing beyond bash, gzip and the usual POSIX tools is needed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$ROOT/scripts/sdk-web-wasm-size.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

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

# expect_ok <name> <expected stdout> -- <command...>
expect_ok() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>&1)"; then
        if [[ "$out" == "$want" ]]; then
            pass "$name"
        else
            fail "$name" "expected: $want"$'\n'"got: $out"
        fi
    else
        fail "$name" "exited non-zero: $out"
    fi
}

# expect_fail <name> <expected text in output> -- <command...>
expect_fail() {
    local name="$1" want="$2" out
    shift 3
    if out="$("$@" 2>&1)"; then
        fail "$name" "expected a failure, got: $out"
    elif [[ "$out" == *"$want"* ]]; then
        pass "$name"
    else
        fail "$name" "expected the error to mention: $want"$'\n'"got: $out"
    fi
}

# expect_usage <name> -- <command...>: fails with status 2 and prints the usage lines.
expect_usage() {
    local name="$1" out status=0
    shift 2
    out="$("$@" 2>&1)" || status=$?
    if [[ "$status" -eq 2 && "$out" == *usage* ]]; then
        pass "$name"
    else
        fail "$name" "expected exit status 2 and a usage message, got status $status: $out"
    fi
}

# expect_silent_fail <name> -- <command...>: fails with status 1 and prints nothing on stdout,
# since what it prints is appended to a job summary as it comes.
expect_silent_fail() {
    local name="$1" out status=0
    shift 2
    out="$("$@" 2>/dev/null)" || status=$?
    if [[ "$status" -eq 1 && -z "$out" ]]; then
        pass "$name"
    else
        fail "$name" "expected exit status 1 and no output, got status $status: $out"
    fi
}

# expect_stdout <name> <exit status> <expected stdout> -- <command...>: exits with the status and
# prints exactly the text on stdout, every newline in it counted, the last one too. What it prints
# on stderr is not counted.
expect_stdout() {
    local name="$1" want_status="$2" want="$3" out status=0
    shift 4
    "$@" >"$WORK/stdout" 2>/dev/null || status=$?
    IFS= read -r -d '' out <"$WORK/stdout" || true
    if [[ "$status" -eq "$want_status" && "$out" == "$want" ]]; then
        pass "$name"
    else
        fail "$name" \
            "expected status $want_status, stdout: $want${NL}got status $status, stdout: $out"
    fi
}

# check <name> <condition command...>: passes when the command succeeds.
check() {
    local name="$1"
    shift
    if "$@"; then pass "$name"; else fail "$name"; fi
}

run() {
    "$SCRIPT" "$@"
}

# Prints 180 KB of text that gzip compresses differently at its default level and at level 9:
# 20000 words from a pool of 3000, picked by a fixed pseudo-random sequence so that the file is
# the same on every machine.
fixture() {
    awk 'BEGIN {
        x = 12345
        for (i = 0; i < 3000; i++) {
            x = (x * 48271) % 2147483647
            word[i] = sprintf("K%07x", x % 268435456)
        }
        for (i = 0; i < 20000; i++) {
            x = (x * 48271) % 2147483647
            sep = (i % 8 == 7) ? "\n" : " "
            printf "%s%s", word[x % 3000], sep
        }
    }'
}

# The size of the file $1 in bytes.
bytes_of() {
    wc -c <"$1" | tr -d ' '
}

# The size of the file $1 after gzip at level $2 (the default level when there is no $2).
gzip_bytes_of() {
    gzip ${2:+"$2"} -c <"$1" | wc -c | tr -d ' '
}

# Writes the sizes file $1 in the work directory with the line $2 and prints its path.
sizes_file() {
    mkdir -p "$WORK/sizes"
    printf '%s\n' "$2" >"$WORK/sizes/$1"
    echo "$WORK/sizes/$1"
}

# Prints the report on the head's sizes $1 and the base's sizes $2.
report_of() {
    run report "$(sizes_file head "$1")" "$(sizes_file base "$2")"
}

# Prints the row labelled $3 of the report on the head's sizes $1 and the base's sizes $2.
row() {
    report_of "$1" "$2" | grep -F "| $3 | "
}

# Runs check on the head's sizes $1 and the base's sizes $2.
check_of() {
    run check "$(sizes_file head "$1")" "$(sizes_file base "$2")"
}

HEAD_SIZES="12347214 4501234"
BASE_SIZES="12339872 4500000"
NL=$'\n'

# The report on the two sizes above: a typical change.
IFS= read -r -d '' FULL_REPORT <<'EOF' || true
### Size of the browser SDK's wasm module

The size of `fedimint_sdk.wasm`, the module that `@fedimint/sdk-web` ships, as it is and after
`gzip -9`.

| | Base | Pull request | Difference (bytes) | Difference (% of base) |
| --- | ---: | ---: | ---: | ---: |
| Module | 12.34 MB (12339872 B) | 12.35 MB (12347214 B) | +7342 | +0.1% |
| Module, gzip -9 | 4.50 MB (4500000 B) | 4.50 MB (4501234 B) | +1234 | +0.0% |

"Base" is the commit of the base branch that the pull request is merged into in the checkout of
this run, and "Pull request" is the result of that merge. Both modules went through the pull
request's `scripts/generate-sdk-web-bindings.sh` and its tools, so a change to that script or to
those tools does not show in the difference.

A difference of a hundred bytes or so in the gzip row is not a change: the generator does not
write the same bytes on every run, even from the same crate.

The check fails when the pull request makes either size grow by more than 5% of the base's.
EOF
FULL_REPORT="${FULL_REPORT%"$NL"}"

# The report on the head's sizes above when there is no base to compare with.
IFS= read -r -d '' NO_BASE_REPORT <<'EOF' || true
### Size of the browser SDK's wasm module

The size of `fedimint_sdk.wasm`, the module that `@fedimint/sdk-web` ships, as it is and after
`gzip -9`.

| | Pull request |
| --- | ---: |
| Module | 12.35 MB (12347214 B) |
| Module, gzip -9 | 4.50 MB (4501234 B) |

There is no size for the base commit to compare with, so the growth of the module is not
checked. The log of the `Test` job says why.
EOF
NO_BASE_REPORT="${NO_BASE_REPORT%"$NL"}"

# Contents, as printf %b reads them, of files that are not a sizes line.
NOT_SIZES=(
    ''
    '\n'
    'twelve megabytes\n'
    '12347214\n'
    '12347214 4501234 7\n'
    '-12347214 4501234\n'
    '12347214.5 4501234\n'
    '0x10 4501234\n'
    '012347214 4501234\n'
    '12347214 04501234\n'
    '1234567890123456 4501234\n'
    '12347214  4501234\n'
    '12347214\t4501234\n'
    ' 12347214 4501234\n'
    '12347214 4501234 \n'
    '12347214 4501234\r\n'
    '12347214 4501234\n\n1 2\n'
    '12347214 4501234\n1 2\n'
    '\n12347214 4501234\n'
)

# A head that is far bigger than any base that the contents of NOT_SIZES could be mistaken for, so
# that a check which took one of them for the base would fail on it.
GROWN_HEAD="50000000 20000000"

# What check prints for a module of 12 MB that grew by 600001 bytes, one byte more than 5%, and for
# a module of 4 MB after gzip -9 that grew by 200001 bytes, one byte more than 5%.
MODULE_LINE="The module grew by 600001 B, from 12.00 MB (12000000 B) to 12.60 MB (12600001 B),"
MODULE_LINE+=" and a pull request may add at most 5% of the base, 600000 B."
GZIP_LINE="The module after gzip -9 grew by 200001 B, from 4.00 MB (4000000 B)"
GZIP_LINE+=" to 4.20 MB (4200001 B), and a pull request may add at most 5% of the base, 200000 B."

echo "measure"

module="$WORK/module.wasm"
fixture >"$module"
check "the fixture compresses differently at gzip's default level and at level 9" \
    test "$(gzip_bytes_of "$module")" != "$(gzip_bytes_of "$module" -9)"
expect_ok "measure prints the size and the size after gzip -9" \
    "$(bytes_of "$module") $(gzip_bytes_of "$module" -9)" -- run measure "$module"

mkdir "$WORK/another place"
cp "$module" "$WORK/another place/a-longer-name-than-the-first.wasm"
expect_ok "the name of the file is no part of the sizes" \
    "$(bytes_of "$module") $(gzip_bytes_of "$module" -9)" -- \
    run measure "$WORK/another place/a-longer-name-than-the-first.wasm"

printf 'abc' >"$WORK/three"
expect_ok "a file of three bytes is three bytes" "3 $(gzip_bytes_of "$WORK/three" -9)" -- \
    run measure "$WORK/three"

: >"$WORK/empty.wasm"
expect_ok "an empty file is measured, not refused" "0 $(gzip_bytes_of "$WORK/empty.wasm" -9)" -- \
    run measure "$WORK/empty.wasm"

expect_silent_fail "a missing module is an error and prints nothing" -- \
    run measure "$WORK/none.wasm"
expect_fail "a missing module is named" "no module at $WORK/none.wasm" -- \
    run measure "$WORK/none.wasm"
expect_fail "a directory is no module" "no module at $WORK" -- run measure "$WORK"

# Root reads a file whatever its mode, so there the case would show nothing.
if [[ "$(id -u)" -ne 0 ]]; then
    cp "$module" "$WORK/unreadable.wasm"
    chmod 000 "$WORK/unreadable.wasm"
    expect_silent_fail "a module that cannot be read is an error and prints nothing" -- \
        run measure "$WORK/unreadable.wasm"
fi

echo
echo "report: a typical change"

expect_ok "the full report, to the letter" "$FULL_REPORT" -- report_of "$HEAD_SIZES" "$BASE_SIZES"
check "a report ends with a newline" \
    test "$(report_of "$HEAD_SIZES" "$BASE_SIZES"; echo x)" = "$FULL_REPORT${NL}x"
check "a report has no trailing whitespace" \
    test -z "$(report_of "$HEAD_SIZES" "$BASE_SIZES" | grep -E '[[:space:]]$' || true)"

echo
echo "report: the difference"

expect_ok "a bigger head adds, with a plus sign" \
    "| Module | 12.00 MB (12000000 B) | 12.06 MB (12060000 B) | +60000 | +0.5% |" -- \
    row "12060000 4012000" "12000000 4000000" "Module"
expect_ok "the gzipped size has a row of its own" \
    "| Module, gzip -9 | 4.00 MB (4000000 B) | 4.01 MB (4012000 B) | +12000 | +0.3% |" -- \
    row "12060000 4012000" "12000000 4000000" "Module, gzip -9"
expect_ok "a smaller head subtracts, with a minus sign" \
    "| Module | 12.00 MB (12000000 B) | 11.94 MB (11940000 B) | -60000 | -0.5% |" -- \
    row "11940000 3988000" "12000000 4000000" "Module"
expect_ok "a smaller gzipped size has a minus sign too" \
    "| Module, gzip -9 | 4.00 MB (4000000 B) | 3.99 MB (3988000 B) | -12000 | -0.3% |" -- \
    row "11940000 3988000" "12000000 4000000" "Module, gzip -9"
expect_ok "an equal head is +0 and +0.0%" \
    "| Module | 12.00 MB (12000000 B) | 12.00 MB (12000000 B) | +0 | +0.0% |" -- \
    row "12000000 4000000" "12000000 4000000" "Module"
expect_ok "an equal gzipped size is +0 and +0.0%" \
    "| Module, gzip -9 | 4.00 MB (4000000 B) | 4.00 MB (4000000 B) | +0 | +0.0% |" -- \
    row "12000000 4000000" "12000000 4000000" "Module, gzip -9"
expect_ok "a few kilobytes of twelve megabytes show in the bytes" \
    "| Module | 12.00 MB (12000000 B) | 12.00 MB (12003072 B) | +3072 | +0.0% |" -- \
    row "12003072 4000000" "12000000 4000000" "Module"
expect_ok "a change too small to show keeps its minus sign" \
    "| Module | 12.00 MB (12000000 B) | 12.00 MB (11999999 B) | -1 | -0.0% |" -- \
    row "11999999 4000000" "12000000 4000000" "Module"
expect_ok "a change too small to show keeps its plus sign" \
    "| Module | 12.00 MB (12000000 B) | 12.00 MB (12000001 B) | +1 | +0.0% |" -- \
    row "12000001 4000000" "12000000 4000000" "Module"

echo
echo "report: the percentage is of the base"

expect_ok "a growth from 10 to 12 MB is +20.0%, not 16.7%" \
    "| Module | 10.00 MB (10000000 B) | 12.00 MB (12000000 B) | +2000000 | +20.0% |" -- \
    row "12000000 5000000" "10000000 4000000" "Module"
expect_ok "a growth from 4 to 5 MB is +25.0%, not 20.0%" \
    "| Module, gzip -9 | 4.00 MB (4000000 B) | 5.00 MB (5000000 B) | +1000000 | +25.0% |" -- \
    row "12000000 5000000" "10000000 4000000" "Module, gzip -9"
expect_ok "a shrinking from 12 to 9 MB is -25.0%, not -33.3%" \
    "| Module | 12.00 MB (12000000 B) | 9.00 MB (9000000 B) | -3000000 | -25.0% |" -- \
    row "9000000 3000000" "12000000 4000000" "Module"
expect_ok "doubling is +100.0%" \
    "| Module | 6.00 MB (6000000 B) | 12.00 MB (12000000 B) | +6000000 | +100.0% |" -- \
    row "12000000 4000000" "6000000 4000000" "Module"
expect_ok "a head of nothing is -100.0%" \
    "| Module | 12.00 MB (12000000 B) | 0.00 MB (0 B) | -12000000 | -100.0% |" -- \
    row "0 20" "12000000 4000000" "Module"

echo
echo "report: the rounding"

expect_ok "a growth of exactly half a tenth of a percent rounds up" \
    "| Module, gzip -9 | 4.00 MB (4000000 B) | 4.01 MB (4010000 B) | +10000 | +0.3% |" -- \
    row "12000000 4010000" "12000000 4000000" "Module, gzip -9"
expect_ok "a shrinking of exactly half a tenth of a percent rounds away from zero" \
    "| Module, gzip -9 | 4.00 MB (4000000 B) | 3.99 MB (3990000 B) | -10000 | -0.3% |" -- \
    row "12000000 3990000" "12000000 4000000" "Module, gzip -9"
expect_ok "a growth just under half a tenth of a percent rounds down" \
    "| Module, gzip -9 | 4.00 MB (4000000 B) | 4.01 MB (4009999 B) | +9999 | +0.2% |" -- \
    row "12000000 4009999" "12000000 4000000" "Module, gzip -9"
expect_ok "a shrinking just under half a tenth of a percent rounds down" \
    "| Module, gzip -9 | 4.00 MB (4000000 B) | 3.99 MB (3990001 B) | -9999 | -0.2% |" -- \
    row "12000000 3990001" "12000000 4000000" "Module, gzip -9"
expect_ok "half a hundredth of a megabyte rounds up" \
    "| Module | 12.35 MB (12345000 B) | 12.35 MB (12345000 B) | +0 | +0.0% |" -- \
    row "12345000 4000000" "12345000 4000000" "Module"
expect_ok "just under half a hundredth of a megabyte rounds down" \
    "| Module | 12.34 MB (12344999 B) | 12.34 MB (12344999 B) | +0 | +0.0% |" -- \
    row "12344999 4000000" "12344999 4000000" "Module"
expect_ok "a megabyte is a million bytes" \
    "| Module | 1.00 MB (1000000 B) | 1.00 MB (1000000 B) | +0 | +0.0% |" -- \
    row "1000000 4000000" "1000000 4000000" "Module"
expect_ok "a hundredth of a megabyte below one keeps its zero" \
    "| Module | 1.01 MB (1010000 B) | 1.01 MB (1010000 B) | +0 | +0.0% |" -- \
    row "1010000 4000000" "1010000 4000000" "Module"

echo
echo "report: a base of zero bytes"

expect_ok "a percentage of nothing is n/a" \
    "| Module | 0.00 MB (0 B) | 12.00 MB (12000000 B) | +12000000 | n/a |" -- \
    row "12000000 4000000" "0 0" "Module"
expect_ok "a percentage of nothing is n/a in the gzipped row too" \
    "| Module, gzip -9 | 0.00 MB (0 B) | 4.00 MB (4000000 B) | +4000000 | n/a |" -- \
    row "12000000 4000000" "0 0" "Module, gzip -9"
expect_ok "no change from nothing is +0.0%" \
    "| Module | 0.00 MB (0 B) | 0.00 MB (0 B) | +0 | +0.0% |" -- \
    row "0 0" "0 0" "Module"
expect_ok "no change from nothing is +0.0% in the gzipped row too" \
    "| Module, gzip -9 | 0.00 MB (0 B) | 0.00 MB (0 B) | +0 | +0.0% |" -- \
    row "0 0" "0 0" "Module, gzip -9"
expect_ok "a base of an empty module has no percentage for the module" \
    "| Module | 0.00 MB (0 B) | 12.00 MB (12000000 B) | +12000000 | n/a |" -- \
    row "12000000 4000000" "0 20" "Module"
expect_ok "a base of an empty module has a percentage for the gzipped module" \
    "| Module, gzip -9 | 0.00 MB (20 B) | 4.00 MB (4000000 B) | +3999980 | +19999900.0% |" -- \
    row "12000000 4000000" "0 20" "Module, gzip -9"

echo
echo "report: the largest sizes"

want="| Module | 0.00 MB (1 B) | 1000000000.00 MB (999999999999999 B) | +999999999999998 |"
expect_ok "15 digits do not overflow" "$want +99999999999999800.0% |" -- \
    row "999999999999999 999999999999999" "1 1" "Module"
want="| Module | 1000000000.00 MB (999999999999999 B) | 0.00 MB (1 B) | -999999999999998 |"
expect_ok "15 digits do not overflow in a shrinking either" "$want -100.0% |" -- \
    row "1 1" "999999999999999 999999999999999" "Module"

echo
echo "report: without a base"

expect_ok "no base file is no base" "$NO_BASE_REPORT" -- \
    run report "$(sizes_file head "$HEAD_SIZES")"
expect_ok "an empty base argument is no base" "$NO_BASE_REPORT" -- \
    run report "$(sizes_file head "$HEAD_SIZES")" ""
expect_ok "a missing base file is no base" "$NO_BASE_REPORT" -- \
    run report "$(sizes_file head "$HEAD_SIZES")" "$WORK/none.txt"
expect_ok "a base that is a directory is no base" "$NO_BASE_REPORT" -- \
    run report "$(sizes_file head "$HEAD_SIZES")" "$WORK"
for content in "${NOT_SIZES[@]}"; do
    printf '%b' "$content" >"$WORK/not-sizes"
    expect_ok "a base holding '$content' is no base" "$NO_BASE_REPORT" -- \
        run report "$(sizes_file head "$HEAD_SIZES")" "$WORK/not-sizes"
done
check "a report without a base ends with a newline" \
    test "$(run report "$(sizes_file head "$HEAD_SIZES")"; echo x)" = "$NO_BASE_REPORT${NL}x"
check "a report without a base has no trailing whitespace" \
    test -z "$(run report "$(sizes_file head "$HEAD_SIZES")" | grep -E '[[:space:]]$' || true)"

echo
echo "report: a head that is no sizes file"

expect_silent_fail "a missing head file is an error and prints nothing" -- \
    run report "$WORK/none.txt" "$(sizes_file base "$BASE_SIZES")"
expect_fail "a missing head file is named" "no sizes file at $WORK/none.txt" -- \
    run report "$WORK/none.txt" "$(sizes_file base "$BASE_SIZES")"
expect_silent_fail "a missing head file is an error without a base too" -- \
    run report "$WORK/none.txt"
expect_fail "a head that is a directory is no sizes file" "no sizes file at $WORK" -- \
    run report "$WORK" "$(sizes_file base "$BASE_SIZES")"
for content in "${NOT_SIZES[@]}"; do
    printf '%b' "$content" >"$WORK/not-sizes"
    expect_silent_fail "a head holding '$content' is an error and prints nothing" -- \
        run report "$WORK/not-sizes" "$(sizes_file base "$BASE_SIZES")"
done
printf '12347214\n' >"$WORK/not-sizes"
expect_fail "a head that is not a sizes line is named" "$WORK/not-sizes is not a sizes file" -- \
    run report "$WORK/not-sizes" "$(sizes_file base "$BASE_SIZES")"
expect_fail "a head that is not a sizes line is an error without a base too" \
    "$WORK/not-sizes is not a sizes file" -- run report "$WORK/not-sizes"
expect_fail "the error says what a sizes file holds" "<bytes> <gzip bytes>" -- \
    run report "$WORK/not-sizes"
expect_silent_fail "a head that is not a sizes line is an error whatever the base" -- \
    run report "$WORK/not-sizes" "$WORK/not-sizes"

echo
echo "report: the end of a sizes line"

printf '12347214 4501234' >"$WORK/no-newline"
expect_ok "a sizes line needs no newline at its end" "$NO_BASE_REPORT" -- \
    run report "$WORK/no-newline"
printf '12347214 4501234\n\n\n' >"$WORK/newlines"
expect_ok "newlines after a sizes line are no matter" "$NO_BASE_REPORT" -- \
    run report "$WORK/newlines"

echo
echo "check: the limit"

expect_stdout "a growth of exactly 5% passes and prints nothing" 0 "" -- \
    check_of "12600000 4000000" "12000000 4000000"
expect_stdout "a growth of exactly 5% after gzip -9 passes and prints nothing" 0 "" -- \
    check_of "12000000 4200000" "12000000 4000000"
expect_stdout "one byte more than 5% fails, with the growth and the allowance" 1 \
    "$MODULE_LINE$NL" -- check_of "12600001 4000000" "12000000 4000000"
expect_stdout "the size after gzip -9 alone growing too much prints its line only" 1 \
    "$GZIP_LINE$NL" -- check_of "12000000 4200001" "12000000 4000000"
expect_stdout "both sizes growing too much print both lines, the module's first" 1 \
    "$MODULE_LINE$NL$GZIP_LINE$NL" -- check_of "12600001 4200001" "12000000 4000000"
expect_stdout "a smaller module does not make up for a bigger one after gzip -9" 1 \
    "$GZIP_LINE$NL" -- check_of "6000000 4200001" "12000000 4000000"
expect_stdout "a smaller size after gzip -9 does not make up for a bigger module" 1 \
    "$MODULE_LINE$NL" -- check_of "12600001 2000000" "12000000 4000000"

echo
echo "check: the allowance is rounded down"

# 5% of 12339872 bytes is 616993.6 bytes.
expect_stdout "a growth of 616993 bytes from 12339872 passes and prints nothing" 0 "" -- \
    check_of "12956865 4000000" "12339872 4000000"
want="The module grew by 616994 B, from 12.34 MB (12339872 B) to 12.96 MB (12956866 B),"
want+=" and a pull request may add at most 5% of the base, 616993 B."
expect_stdout "a growth of 616994 bytes from 12339872 fails, and 616993 may be added" 1 \
    "$want$NL" -- check_of "12956866 4000000" "12339872 4000000"

echo
echo "check: no growth"

expect_stdout "a large shrinking passes and prints nothing" 0 "" -- \
    check_of "6000000 2000000" "12000000 4000000"
expect_stdout "no change passes and prints nothing" 0 "" -- \
    check_of "12000000 4000000" "12000000 4000000"

echo
echo "check: a base of zero bytes"

want="The module grew by 1 B, from 0.00 MB (0 B) to 0.00 MB (1 B),"
want+=" and a pull request may add at most 5% of the base, 0 B.$NL"
want+="The module after gzip -9 grew by 1 B, from 0.00 MB (0 B) to 0.00 MB (1 B),"
want+=" and a pull request may add at most 5% of the base, 0 B.$NL"
expect_stdout "a base of 0 bytes allows 0 bytes, so any growth from it fails" 1 "$want" -- \
    check_of "1 1" "0 0"
expect_stdout "no change from 0 bytes passes and prints nothing" 0 "" -- \
    check_of "0 0" "0 0"

echo
echo "check: the largest sizes"

want="The module grew by 999999999999998 B, from 0.00 MB (1 B)"
want+=" to 1000000000.00 MB (999999999999999 B),"
want+=" and a pull request may add at most 5% of the base, 0 B.$NL"
want+="The module after gzip -9 grew by 999999999999998 B, from 0.00 MB (1 B)"
want+=" to 1000000000.00 MB (999999999999999 B),"
want+=" and a pull request may add at most 5% of the base, 0 B.$NL"
expect_stdout "15 digits do not overflow" 1 "$want" -- \
    check_of "999999999999999 999999999999999" "1 1"
want="The module grew by 900000000000000 B, from 100000000.00 MB (99999999999999 B)"
want+=" to 1000000000.00 MB (999999999999999 B),"
want+=" and a pull request may add at most 5% of the base, 4999999999999 B.$NL"
want+="The module after gzip -9 grew by 900000000000000 B, from 100000000.00 MB (99999999999999 B)"
want+=" to 1000000000.00 MB (999999999999999 B),"
want+=" and a pull request may add at most 5% of the base, 4999999999999 B.$NL"
expect_stdout "15 digits do not overflow with a base of 14 digits either" 1 "$want" -- \
    check_of "999999999999999 999999999999999" "99999999999999 99999999999999"
expect_stdout "equal sizes of 15 digits pass and print nothing" 0 "" -- \
    check_of "999999999999999 999999999999999" "999999999999999 999999999999999"
expect_stdout "a shrinking from 15 digits passes and prints nothing" 0 "" -- \
    check_of "1 1" "999999999999999 999999999999999"

echo
echo "check: without a base"

expect_stdout "no base file is no base" 0 "" -- \
    run check "$(sizes_file head "$GROWN_HEAD")"
expect_stdout "an empty base argument is no base" 0 "" -- \
    run check "$(sizes_file head "$GROWN_HEAD")" ""
expect_stdout "a missing base file is no base" 0 "" -- \
    run check "$(sizes_file head "$GROWN_HEAD")" "$WORK/none.txt"
expect_stdout "a base that is a directory is no base" 0 "" -- \
    run check "$(sizes_file head "$GROWN_HEAD")" "$WORK"
for content in "${NOT_SIZES[@]}"; do
    printf '%b' "$content" >"$WORK/not-sizes"
    expect_stdout "a base holding '$content' is no base" 0 "" -- \
        run check "$(sizes_file head "$GROWN_HEAD")" "$WORK/not-sizes"
done
check "a check without a base prints nothing on stderr either" \
    test -z "$(run check "$(sizes_file head "$GROWN_HEAD")" "$WORK/none.txt" 2>&1)"

echo
echo "check: a head that is no sizes file"

expect_silent_fail "a missing head file is an error and prints nothing" -- \
    run check "$WORK/none.txt" "$(sizes_file base "$BASE_SIZES")"
expect_fail "a missing head file is named" "no sizes file at $WORK/none.txt" -- \
    run check "$WORK/none.txt" "$(sizes_file base "$BASE_SIZES")"
expect_silent_fail "a missing head file is an error without a base too" -- \
    run check "$WORK/none.txt"
expect_silent_fail "a head that is a directory is an error and prints nothing" -- \
    run check "$WORK" "$(sizes_file base "$BASE_SIZES")"
expect_fail "a head that is a directory is named" "no sizes file at $WORK" -- \
    run check "$WORK" "$(sizes_file base "$BASE_SIZES")"
for content in "${NOT_SIZES[@]}"; do
    printf '%b' "$content" >"$WORK/not-sizes"
    expect_silent_fail "a head holding '$content' is an error and prints nothing" -- \
        run check "$WORK/not-sizes" "$(sizes_file base "$BASE_SIZES")"
    expect_fail "a head holding '$content' is named in the error" \
        "$WORK/not-sizes is not a sizes file; it must hold one line, <bytes> <gzip bytes>" -- \
        run check "$WORK/not-sizes" "$(sizes_file base "$BASE_SIZES")"
done
expect_silent_fail "a head that is not a sizes line is an error whatever the base" -- \
    run check "$WORK/not-sizes" "$WORK/not-sizes"

echo
echo "check: what it prints"

check "a failing check ends each line with a newline" \
    test "$(check_of "12600001 4200001" "12000000 4000000" || true; echo x)" = \
    "$MODULE_LINE$NL$GZIP_LINE${NL}x"
check "a failing check has no trailing whitespace" \
    test -z "$(check_of "12600001 4200001" "12000000 4000000" | grep -E '[[:space:]]$' || true)"
check "a passing check prints nothing, not even a newline" \
    test "$(check_of "12000000 4000000" "12000000 4000000"; echo x)" = x

echo
echo "usage"

expect_usage "no arguments is a usage error" -- run
expect_usage "an unknown command is a usage error" -- run size "$module"
expect_usage "an empty command is a usage error" -- run ""
expect_usage "--help is a usage error" -- run --help
expect_usage "measure without a module is a usage error" -- run measure
expect_usage "measure with two modules is a usage error" -- run measure "$module" "$module"
expect_usage "report without a file is a usage error" -- run report
expect_usage "report with three files is a usage error" -- \
    run report "$(sizes_file head "$HEAD_SIZES")" "$(sizes_file base "$BASE_SIZES")" \
    "$(sizes_file extra "$BASE_SIZES")"
expect_usage "check without a file is a usage error" -- run check
expect_usage "check with three files is a usage error" -- \
    run check "$(sizes_file head "$HEAD_SIZES")" "$(sizes_file base "$BASE_SIZES")" \
    "$(sizes_file extra "$BASE_SIZES")"
expect_fail "the usage names measure" "usage: scripts/sdk-web-wasm-size.sh measure <module>" -- run
expect_fail "the usage names report" \
    "usage: scripts/sdk-web-wasm-size.sh report <head sizes file> [<base sizes file>]" -- run
expect_fail "the usage names check" \
    "usage: scripts/sdk-web-wasm-size.sh check <head sizes file> [<base sizes file>]" -- run

echo
echo "measure and report together"

head -c 100000 "$module" >"$WORK/smaller.wasm"
run measure "$module" >"$WORK/head-size"
run measure "$WORK/smaller.wasm" >"$WORK/base-size"
growth=$(($(bytes_of "$module") - $(bytes_of "$WORK/smaller.wasm")))
check "the sizes measure prints are the sizes report reads" \
    grep -qF "| +$growth | +" <(run report "$WORK/head-size" "$WORK/base-size")
check "a module that shrank shows its loss" \
    grep -qF "| -$growth | -" <(run report "$WORK/base-size" "$WORK/head-size")
check "the sizes measure prints are the sizes check reads" \
    grep -qF "The module grew by $growth B," <(run check "$WORK/head-size" "$WORK/base-size")
expect_stdout "a module that shrank passes the check" 0 "" -- \
    run check "$WORK/base-size" "$WORK/head-size"

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
