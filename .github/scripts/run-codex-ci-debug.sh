#!/usr/bin/env bash
set -euo pipefail

require_env() {
  local name="$1"
  if [ -z "${!name:-}" ]; then
    echo "Missing required environment variable: ${name}" >&2
    exit 1
  fi
}

require_env BACKPORT_TOKEN
require_env CI_DEBUG_RUN_ID
require_env CI_DEBUG_RUN_URL
require_env GITHUB_ACTOR
require_env GITHUB_EVENT_NAME
require_env GITHUB_EVENT_PATH
require_env GITHUB_REPOSITORY
require_env GITHUB_WORKSPACE
require_env PPQ_KEY
require_env PPQ_MODEL
require_env RUNNER_TEMP

# Workflow-dispatch runs intentionally leave some CI_DEBUG_* values empty.
# Only require the fields needed to identify and fetch the target run.

agent_root="${RUNNER_TEMP}/codex-ci-debug"
agent_home="${agent_root}/home"
codex_home="${agent_root}/codex-home"
context_dir="${agent_root}/context"

rm -rf "${agent_root}"
mkdir -p \
  "${agent_home}" \
  "${codex_home}" \
  "${context_dir}"

path_toml=$(jq -Rn --arg value "${PATH}" '$value')

cat > "${codex_home}/config.toml" <<EOF
model = "${PPQ_MODEL}"
model_provider = "ppq"
sandbox_mode = "danger-full-access"
approval_policy = "never"


[model_providers.ppq]
name = "PPQ"
base_url = "https://api.ppq.ai/v1"
env_key = "PPQ_KEY"
wire_api = "responses"


[shell_environment_policy]
inherit = "all"
ignore_default_excludes = true
set = { PATH = ${path_toml} }
include_only = [
  "PATH",
  "HOME",
  "CODEX_HOME",
  "GH_TOKEN",
  "GITHUB_API_URL",
  "GITHUB_ACTOR",
  "GITHUB_EVENT_NAME",
  "GITHUB_GRAPHQL_URL",
  "GITHUB_REPOSITORY",
  "GITHUB_RUN_ID",
  "GITHUB_SERVER_URL",
  "GITHUB_WORKSPACE",
  "RUNNER_TEMP",
  "CI_DEBUG_RUN_ID",
  "CI_DEBUG_RUN_URL",
  "CI_DEBUG_WORKFLOW_NAME",
  "CI_DEBUG_WORKFLOW_EVENT",
  "CI_DEBUG_HEAD_BRANCH",
  "CI_DEBUG_HEAD_SHA",
  "IN_NIX_SHELL",
  "REPO_ROOT",
  "NODE_*",
  "NPM_*",
  "npm_*",
  "PNPM_*",
  "COREPACK_*",
  "PLAYWRIGHT_*",
  "CARGO_*",
  "RUST*",
  "FM_*",
  "NIX_*",
  "NIXPKGS_*",
  "CC*",
  "CXX*",
  "CFLAGS*",
  "CPPFLAGS",
  "PKG_CONFIG*",
  "LD",
  "LD_*",
  "LD_LIBRARY_PATH",
  "CONFIG_SHELL",
  "SHELL",
  "LANG",
  "LC_*",
  "LOCALE_ARCHIVE",
  "TERM",
  "TERMINFO_DIRS",
  "TMPDIR",
  "TEMP",
  "TMP",
  "TEMPDIR",
  "PYTHON*",
  "XDG_CONFIG_DIRS",
  "XDG_DATA_DIRS",
  "SOURCE_DATE_EPOCH",
  "SSL_CERT_FILE",
  "NIX_SSL_CERT_FILE",
]
EOF

jq '{
  action,
  repository: env.GITHUB_REPOSITORY,
  event_name: env.GITHUB_EVENT_NAME,
  actor: env.GITHUB_ACTOR,
  workflow_run: .workflow_run,
  inputs: .inputs
}' "${GITHUB_EVENT_PATH}" > "${context_dir}/event.json"

cat > "${context_dir}/prompt.md" <<'EOF'
You are fedimint-bot, an automation agent for failed fedimint-sdk CI runs.
A GitHub Actions workflow failed on the main branch. Investigate the failure
and either comment on the culprit pull request, document unresolved findings
in a GitHub issue, or open a draft pull request with a small, well-scoped fix.

fedimint-sdk is a TypeScript monorepo (pnpm workspaces) providing SDKs for
building web applications on Fedimint, wrapping the Rust client compiled to
WebAssembly.

Target run:
- Run id: $CI_DEBUG_RUN_ID
- Run URL: $CI_DEBUG_RUN_URL
- Workflow: $CI_DEBUG_WORKFLOW_NAME
- Event: $CI_DEBUG_WORKFLOW_EVENT
- Head branch: $CI_DEBUG_HEAD_BRANCH
- Head SHA: $CI_DEBUG_HEAD_SHA

Use the GitHub event payload at:
$RUNNER_TEMP/codex-ci-debug/context/event.json

Available tools:
- `gh`, authenticated as fedimint-bot via `GH_TOKEN`;
- `git`;
- normal shell tools including `bash`, `curl`, `jq`, `rg`, `sed`, and `awk`;
- the repository's Nix development shell is active, including `node`, `pnpm`,
  and the devimint test environment used by the integration tests.

Baseline investigation:
- Start with `gh run view "$CI_DEBUG_RUN_ID" --repo "$GITHUB_REPOSITORY"` and
  `gh api repos/:owner/:repo/actions/runs/$CI_DEBUG_RUN_ID/jobs --paginate`.
- Find the real failed job/step. Fetch focused logs for the failing jobs with
  `gh run view --job <job-id> --log`. Search for `FAIL`, `error`, `ELIFECYCLE`,
  `timed out`, and the failing test or package name.
- Identify the commit or PR that landed on main before the failure
  (`git log`, `gh pr list --search <sha>`) and compare the failure with those
  changes before deciding whether this is a repository-level failure or a
  legitimate bug in the merged PR.
- Check recent history for the same failure using `gh run list`,
  `gh issue list/search`, and `gh pr list/search` before opening duplicate
  issues or PRs.
- Always search for existing GitHub issues tracking the same failure before
  opening a new issue. Update the existing issue instead when there is a clear
  match.

Common failure categories in this repository:
- The "Changesets" workflow handles versioning and npm publishing — failures
  are often npm auth/registry errors, changeset misconfiguration, or a package
  that no longer builds. Treat clear upstream registry HTTP 5xx failures as
  transient unless they recur enough to justify a retry/caching fix.
- Integration tests run vitest against a real devimint federation and can be
  flaky for timing reasons; a flaky failure still needs to be debugged and
  fixed when there is a high-confidence cause.
- Docs deployment failures are usually VitePress build errors introduced by a
  recently merged docs or API change.

Decision rules:
- Open a draft PR only when the fix is concrete, low-risk, and can be validated
  with a focused command. Create a branch named
  `fedimint-bot/ci-debug-<short-topic>-$CI_DEBUG_RUN_ID`.
- Do not open an issue merely because a failure is flaky; only when the
  investigation cannot establish a high-confidence cause or actionable fix,
  when the cause is external/operational and needs tracking rather than a code
  change, or when maintainer input is required. Include the failing run URL,
  failed job and step, concise log excerpts, and a concrete next action.
- Reuse or update an existing issue/PR when one already clearly tracks the same
  failure. Avoid duplicate issues.
- If the failure is caused by a legitimate bug in a recently merged PR, comment
  on that PR with the failed run URL, failing job and step, concise evidence,
  and what needs to be fixed in a follow-up.
- Run `pnpm lint:fix` after code changes when available, plus the smallest
  useful check (`pnpm typecheck`, `pnpm build`, or a targeted vitest run).
  Report any checks that were skipped and why in the issue or PR.
- Avoid broad refactors and do not change unrelated code.
- Do not mention secrets or environment variable values in comments, issues,
  commits, branches, or PR descriptions.
- Before finishing, comment on the culprit PR, open or update a GitHub issue,
  or open a draft PR. If you cannot complete one of those actions, open an issue
  explaining the blocker.
EOF

envsubst < "${context_dir}/prompt.md" > "${context_dir}/prompt.expanded.md"

export HOME="${agent_home}"
export CODEX_HOME="${codex_home}"
export GH_TOKEN="${BACKPORT_TOKEN}"

git config --global --add safe.directory "${GITHUB_WORKSPACE}"
git config --global user.name fedimint-bot
git config --global user.email fedimint-bot@users.noreply.github.com
gh auth status >/dev/null
gh auth setup-git

cd "${GITHUB_WORKSPACE}"
exec codex exec \
  --ephemeral \
  --dangerously-bypass-approvals-and-sandbox \
  "$(cat "${context_dir}/prompt.expanded.md")"
