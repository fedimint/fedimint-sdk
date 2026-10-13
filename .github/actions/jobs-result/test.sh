#!/usr/bin/env bash
#
# Tests jobs-result.sh, which decides whether the check that stands for a whole workflow passes.
#
#   .github/actions/jobs-result/test.sh
#
# Each case runs the script with the `needs` JSON that GitHub would give and with what the
# detection job answered, and compares the exit status and what the script prints. Nothing beyond
# bash and jq is needed, and the whole run takes well under a second. The action (action.yml) runs
# it as its first step, so that a check that does not work fails the job that uses it instead of
# passing whatever happened.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$HERE/jobs-result.sh"

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

if ! command -v jq >/dev/null; then
    echo "jq is needed to run this test." >&2
    exit 1
fi

# needs <job=result>...: the JSON that GitHub gives for `toJSON(needs)`, with an entry for each job.
needs() {
    local json="" pair
    for pair in "$@"; do
        json="${json:+$json,}\"${pair%%=*}\":{\"result\":\"${pair#*=}\",\"outputs\":{}}"
    done
    printf '{%s}' "$json"
}

# run <ran> <needs-json> [VAR=value...]: runs the script with the answer of the detection job and
# the `needs` JSON, the detection job being `changes` unless a VAR=value argument says otherwise.
# The status goes to $status and everything the script printed to $out.
run() {
    local ran="$1" json="$2"
    shift 2
    status=0
    out="$(env RAN="$ran" NEEDS="$json" DETECTION_JOB=changes "$@" "$BASH" "$SCRIPT" 2>&1)" ||
        status=$?
}

# passes <name> <lines> <ran> <needs-json> [VAR=value...]: the script succeeds and prints exactly
# <lines>, the line of each job and nothing else.
passes() {
    local name="$1" lines="$2"
    shift 2
    run "$@"
    if [[ "$status" -ne 0 ]]; then
        fail "$name" "exited with status $status: $out"
    elif [[ "$out" != "$lines" ]]; then
        fail "$name" "expected the output: $lines"$'\n'"got: $out"
    else
        pass "$name"
    fi
}

# fails <name> <text> <ran> <needs-json> [VAR=value...]: the script exits with status 1 and its
# last line is the only ::error:: line, which says <text>.
fails() {
    local name="$1" text="$2" rest count
    shift 2
    run "$@"
    rest="${out//::error::/}"
    count=$(((${#out} - ${#rest}) / 9))
    if [[ "$status" -ne 1 ]]; then
        fail "$name" "expected status 1, got $status: $out"
    elif [[ "$count" -ne 1 || "${out##*$'\n'}" != ::error::* ]]; then
        fail "$name" "expected one ::error:: line, as the last line, got: $out"
    elif [[ "$out" != *"$text"* ]]; then
        fail "$name" "expected the error to say: $text"$'\n'"got: $out"
    else
        pass "$name"
    fi
}

# prints <name> <output> <ran> <needs-json> [VAR=value...]: the script prints exactly <output>,
# whatever its status.
prints() {
    local name="$1" want="$2"
    shift 2
    run "$@"
    if [[ "$out" == "$want" ]]; then
        pass "$name"
    else
        fail "$name" "expected the output: $want"$'\n'"got: $out"
    fi
}

# says <name> <text>: the last run printed <text>.
says() {
    if [[ "$out" == *"$2"* ]]; then
        pass "$1"
    else
        fail "$1" "expected the output to say: $2"$'\n'"got: $out"
    fi
}

# omits <name> <text>: the last run did not print <text>.
omits() {
    if [[ "$out" != *"$2"* ]]; then
        pass "$1"
    else
        fail "$1" "expected the output not to say: $2"$'\n'"got: $out"
    fi
}

echo "a run that had something to check passes when every job succeeded"

passes "every job succeeded" $'changes: success\nnative: success\ntest: success' \
    true "$(needs changes=success native=success test=success)"
passes "a job name with a dash" $'changes: success\npins-agree: success' \
    true "$(needs pins-agree=success changes=success)"
passes "the detection job can have another name" $'detect: success\ntest: success' \
    true "$(needs test=success detect=success)" DETECTION_JOB=detect
passes "the outputs of the jobs do not matter" $'changes: success\nnative: success' \
    true '{"changes":{"result":"success","outputs":{"run":"true"}},"native":{"result":"success"}}'

echo
echo "a run that had nothing to check passes when every job was skipped"

passes "every other job was skipped" $'changes: success\nnative: skipped\ntest: skipped' \
    false "$(needs changes=success native=skipped test=skipped)"
passes "a job name with a dash" $'changes: success\npins-agree: skipped' \
    false "$(needs pins-agree=skipped changes=success)"

echo
echo "the jobs are printed in every run, sorted by name"

passes "whatever the order they came in" \
    $'aar: success\napk: success\nbindings: success\nchanges: success\nnative: success' \
    true "$(needs native=success changes=success bindings=success apk=success aar=success)"
lines=$'changes: success\nnative: failure\ntest: success'
prints "a failing run prints the jobs before the error" \
    "$lines"$'\n::error::native is failure, expected success.' \
    true "$(needs test=success native=failure changes=success)"
lines=$'changes: success\nnative: skipped\ntest: success'
prints "a run that had nothing to check prints the jobs before the error" \
    "$lines"$'\n::error::test is success, expected skipped.' \
    false "$(needs test=success native=skipped changes=success)"
lines=$'changes: success\ntest: (no result)'
prints "a job without a result is printed as such" \
    "$lines"$'\n::error::test is (no result), expected success.' \
    true '{"test":{"outputs":{}},"changes":{"result":"success"}}'

echo
echo "a run that had something to check fails when a job is not a success"

fails "a failed job" "native is failure, expected success" \
    true "$(needs changes=success native=failure test=success)"
omits "only the job that failed is named" "test is"
fails "a cancelled job" "test is cancelled, expected success" \
    true "$(needs changes=success native=success test=cancelled)"
fails "a skipped job, as when a job it needs failed or the workflow it calls was refused" \
    "native is skipped, expected success" \
    true "$(needs changes=success native=skipped test=success)"
fails "a result that is not one of the four" "test is unknown, expected success" \
    true "$(needs changes=success native=success test=unknown)"
fails "a job without a result" "test is (no result), expected success" \
    true '{"changes":{"result":"success"},"test":{"outputs":{}}}'
fails "a job that is not an object" "test is (no result), expected success" \
    true '{"changes":{"result":"success"},"test":"success"}'
fails "a job with a dash in its name" "pins-agree is failure, expected success" \
    true "$(needs changes=success pins-agree=failure)"
want="aar is skipped, expected success; apk is skipped, expected success; "
want+="bindings is skipped, expected success; native is failure, expected success."
fails "several jobs that are not as they have to be, each of them named" "$want" \
    true "$(needs changes=success native=failure bindings=skipped apk=skipped aar=skipped)"
omits "the job that is as it has to be is not named" "changes is"

echo
echo "a run that had nothing to check fails when a job is not skipped"

fails "a job that succeeded" "test is success, expected skipped" \
    false "$(needs changes=success native=skipped test=success)"
omits "only the job that ran is named" "native is"
fails "a job that failed" "test is failure, expected skipped" \
    false "$(needs changes=success native=skipped test=failure)"
fails "a job that was cancelled" "test is cancelled, expected skipped" \
    false "$(needs changes=success native=skipped test=cancelled)"
fails "several jobs that ran, each of them named" \
    "native is success, expected skipped; test is failure, expected skipped." \
    false "$(needs changes=success native=success test=failure)"

echo
echo "the detection job"

fails "a detection job that failed" "changes is failure, expected success" \
    '' "$(needs changes=failure native=skipped test=skipped)"
says "its empty answer is named with it" 'the answer of changes is "", expected true or false'
fails "a detection job that was cancelled" "changes is cancelled, expected success" \
    '' "$(needs changes=cancelled native=skipped test=skipped)"
fails "a detection job that was skipped" "changes is skipped, expected success" \
    '' "$(needs changes=skipped native=skipped test=skipped)"
fails "a detection job that failed although the answer says true" \
    "changes is failure, expected success" \
    true "$(needs changes=failure native=success test=success)"
fails "a detection job that failed although the answer says false" \
    "changes is failure, expected success" \
    false "$(needs changes=failure native=skipped test=skipped)"
fails "a detection job that is not among the needed jobs" \
    "changes is not among the needed jobs, expected success" \
    true "$(needs native=success test=success)"
fails "a detection job that is looked up by the name it is given" \
    "detect is not among the needed jobs, expected success" \
    true "$(needs changes=success test=success)" DETECTION_JOB=detect
fails "a detection job without a name" "the name of the detection job is empty" \
    true "$(needs changes=success test=success)" DETECTION_JOB=
fails "no job but the detection job" "changes is the only needed job, so nothing is checked" \
    true "$(needs changes=success)"
fails "a detection job that is missing from a needs of one job" \
    "native is not among the needed jobs, expected success" \
    true "$(needs changes=success)" DETECTION_JOB=native

echo
echo "the answer of the detection job"

fails "an empty answer" 'the answer of changes is "", expected true or false' \
    '' "$(needs changes=success native=skipped test=success)"
omits "a skipped job is not judged without an answer" "native is"
omits "a job that succeeded is not judged without an answer" "test is"
fails "an answer that is neither true nor false" \
    'the answer of changes is "yes", expected true or false' \
    yes "$(needs changes=success native=success test=success)"
fails "an answer in capitals" 'the answer of changes is "True", expected true or false' \
    True "$(needs changes=success native=success test=success)"
fails "an answer with a space" 'the answer of changes is " true", expected true or false' \
    ' true' "$(needs changes=success native=success test=success)"

echo
echo "input that is not the needs context"

fails "empty input" "needs is not a JSON object with an entry for each needed job" true ''
fails "text that is not JSON" "needs is not a JSON object" true 'not json'
fails "a JSON array" "needs is not a JSON object" true '[]'
fails "JSON null" "needs is not a JSON object" true 'null'
fails "a JSON string" "needs is not a JSON object" true '"success"'
fails "a JSON number" "needs is not a JSON object" true '5'
fails "an empty object" "needs is not a JSON object" true '{}'
fails "two JSON values" "needs is not a JSON object" \
    true '{"changes":{"result":"success"}} {"changes":{"result":"success"}}'
prints "input that is not JSON prints no job" \
    '::error::needs is not a JSON object with an entry for each needed job.' true 'not json'

echo
echo "$passed passed, $failed failed"
[[ "$failed" -eq 0 ]]
