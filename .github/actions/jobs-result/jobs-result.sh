#!/usr/bin/env bash
#
# Decides whether the check that stands for a whole workflow passes, from the way the jobs it
# needs ended. It prints one line for each job, `<job>: <result>`, sorted by the name of the job,
# and fails with a single `::error::` line when a job is not as it has to be. The composite action
# next to this script (action.yml) runs it.
#
#   NEEDS='<json>' RAN=true DETECTION_JOB=changes .github/actions/jobs-result/jobs-result.sh
#
# The environment:
#
#   NEEDS          The JSON that GitHub gives for `toJSON(needs)`: an object with an entry for each
#                  job the check needs, each with its `result` (`success`, `failure`, `cancelled`
#                  or `skipped`) and its `outputs`.
#   RAN            What the detection job answered, `needs.<detection job>.outputs.run`: `true`
#                  when the workflow has something to check, `false` when it has not, and empty
#                  when the detection job did not get as far as answering.
#   DETECTION_JOB  The name of the detection job, which has to be one of the jobs in NEEDS.
#
# The check passes in two situations and in no other:
#
#   1. The detection job succeeded and answered `true`, and every other job succeeded.
#   2. The detection job succeeded and answered `false`, and every other job was skipped.
#
# Everything else fails: a job that failed or was cancelled, a job that ran although the detection
# job answered `false`, and a job that was skipped although it answered `true`. The last one is
# easy to miss. GitHub skips a job when a job it needs did not succeed, and it also shows a job as
# skipped when the reusable workflow that the job calls is refused, and branch protection counts a
# skipped check as passed. So a skipped job proves nothing when the workflow has something to
# check, and only `success` is accepted then.
#
# A detection job that did not succeed, one that is not among the needed jobs, an answer that is
# neither `true` nor `false`, and a NEEDS that is not a JSON object with an entry for each job fail
# as well, and so does a NEEDS with no job but the detection job: nothing is checked then, which
# can only be a mistake.
#
# The error line names every job that is not as it has to be, its result and the result it should
# have had.
set -euo pipefail

fail() {
    echo "::error::$*"
    exit 1
}

detection="${DETECTION_JOB:-}"
ran="${RAN:-}"
needs="${NEEDS:-}"

[[ -n "$detection" ]] || fail "the name of the detection job is empty."

# `--argjson` takes exactly one JSON value and refuses anything else.
if ! jq -e -n --argjson needs "$needs" '($needs | type) == "object" and ($needs | length) > 0' \
    >/dev/null 2>&1; then
    fail "needs is not a JSON object with an entry for each needed job."
fi

jq -r -n --argjson needs "$needs" '
    $needs | keys[] as $job
    | "\($job): \(($needs[$job] | try .result catch null) // "(no result)")"'

# One text for everything that is wrong, empty when nothing is. The jobs are looked at in the order
# they are printed in. When the answer is neither `true` nor `false` it is not known what the other
# jobs had to be, so they are not judged.
problems="$(jq -r -n --argjson needs "$needs" --arg detection "$detection" --arg ran "$ran" '
    def outcome: (try .result catch null) // "(no result)";
    (if $ran == "true" then "success"
     elif $ran == "false" then "skipped"
     else null end) as $expected
    | [
        (if $needs | has($detection) then
            ($needs[$detection] | outcome) as $result
            | if $result == "success" then empty
              else "\($detection) is \($result), expected success" end
         else
            "\($detection) is not among the needed jobs, expected success"
         end),
        (if $expected == null then
            "the answer of \($detection) is \($ran | @json), expected true or false"
         else
            ($needs | keys[] | select(. != $detection)) as $job
            | ($needs[$job] | outcome) as $result
            | if $result == $expected then empty
              else "\($job) is \($result), expected \($expected)" end
         end),
        (if ($needs | length) == 1 and ($needs | has($detection)) then
            "\($detection) is the only needed job, so nothing is checked"
         else
            empty
         end)
      ]
    | join("; ")')"

if [[ -n "$problems" ]]; then
    fail "$problems."
fi
