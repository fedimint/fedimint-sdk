<script setup>
import packageJson from '../../../../../package.json'

const nodeVersion = packageJson.engines.node
const packageManager = packageJson.packageManager
</script>

# Contributing

Thanks for your interest in contributing to the Fedimint Sdk! Please take a moment to review this document **before submitting a pull request.**

## Overview

This guide is intended to help you get started with contributing. By following these steps, you will understand the development process and workflow.

:::warning
**Please ask first before starting work on any significant new features. This includes things like adding new services, features, or changing the behavior of existing features.**

<!-- It's never a fun experience to have your pull request declined after investing time and effort into a new feature. To avoid this from happening, we request that contributors first create a [feature request](https://github.com/wevm/wagmi/discussions/new?category=ideas) to discuss any API changes or significant new ideas. -->

:::

## Dependency on Fedimint Client WASM

To call Rust side functions, this SDK depends on a WASM binary built from the `fedimint-client-wasm` crate in the [Fedimint repository](https://github.com/fedimint/fedimint/tree/master/fedimint-client-wasm).

**Important:**

- This WASM binary is required for the SDK to function.
- It is built using the source code from the `fedimint` repository, not this SDK repository.
- The specific revision of the `fedimint` repository used is defined in `flake.nix`.

### Updating the WASM Binary

If you have made changes to the Rust side (in the `fedimint` repo) and need to update the WASM binary used by this SDK:

1. Update the `fedimint-wasm` input in [flake.nix](https://github.com/fedimint/fedimint-sdk/blob/main/flake.nix) (around line 9) to point to the new commit/revision.
2. Build the WASM binary (see instructions below).

## Set up Nix

Fedimint uses Nix for managing the development environment. It is **highly recommended** to use Nix to ensure you have the correct tools and versions.

For detailed instructions on setting up Nix, see [Nix Setup](./nix_setup.md).

Once Nix is installed, enter the development shell:

```bash
nix develop
```

## Building the WASM Binary

After setting up Nix and entering the shell, you must build the WASM binary:

```bash
pnpm build:wasm
```

This command will build the `fedimint-client-wasm` crate and place the resulting binary in the `packages/wasm-bundler` folder.

## 1. Cloning the repository

To start contributing to the project, clone it to your local machine using git:

```bash
git clone https://github.com/fedimint/fedimint-sdk.git
```

Or the [GitHub CLI](https://cli.github.com):

```bash
gh repo clone fedimint/fedimint-sdk
```

## 2. Installing Node.js and pnpm

Fedimint Sdk uses Node.js with [pnpm workspaces](https://pnpm.io/workspaces) to manage multiple projects. You can run the following command in your terminal to check your local Node.js version.

```bash
node -v
```

If **`node@{{nodeVersion}}`** is not installed, you can install via [fnm](https://github.com/Schniz/fnm) (recommended) or from the [official website](https://nodejs.org).

Once Node.js is installed, run the following to install [Corepack](https://nodejs.org/api/corepack.html). Corepack automatically installs and manages **`{{packageManager}}`**.

```bash
corepack enable
```

## 3. Installing dependencies

Once in the project's root directory, run the following command to install pnpm (via Corepack) and the project's dependencies:

```bash
pnpm install
```

After the install completes, pnpm links packages across the project for development and [git hooks](https://github.com/toplenboren/simple-git-hooks) are set up.

## 4. Running the dev playgrounds

To start the local development playgrounds, run one of the following commands. These commands run playground apps, located at `./playgrounds`, that are set up for trying out code while making changes.

```bash
pnpm dev              # aliased to `pnpm dev:core`
pnpm dev:core         # `@fedimint/core` + Vite + React app
pnpm dev:next         # `@fedimint/core` + Next.js app
# pnpm dev:react      # TBD
pnpm dev:bare         # HTML + VanillaJS app (no framework)
```

Once a playground dev server is running, you can make changes to any of the package source files (e.g. `packages/react`) and it will automatically update the playground.

## 5. Running the test suite

Fedimint web SDK uses [Vitest](https://vitest.dev) to run tests.
See the [testing docs](/core/dev/testing) for more information.

## 6. Writing documentation

Documentation is crucial to helping developers of all experience levels use Fedimint Sdk. We use [VitePress](https://vitepress.dev) for the documentation site (located at `./docs`). To start the site in dev mode, run:

```bash
pnpm docs:dev
```

Try to keep documentation brief and use plain language so folks of all experience levels can understand. If you think something is unclear or could be explained better, you are welcome to open a pull request.

## 7. Submitting a pull request

When you're ready to submit a pull request, you can follow these naming conventions:

- Pull request titles use the [Imperative Mood](https://en.wikipedia.org/wiki/Imperative_mood) (e.g., `Add something`, `Fix something`).
- [Changesets](#versioning) use past tense verbs (e.g., `Added something`, `Fixed something`).

When you submit a pull request, GitHub will automatically lint, build, and test your changes. If you see an ❌, it's most likely a bug in your code. Please, inspect the logs through the GitHub UI to find the cause.

**Please make sure that "Allow edits from maintainers" is enabled so the core team can make updates to your pull request if necessary.**

## 8. Versioning

When adding new features or fixing bugs, we'll need to bump the package versions. We use [Changesets](https://github.com/changesets/changesets) to do this.

::: tip
Only changes to the codebase that affect the public API or existing behavior (e.g. bugs) need changesets.
:::

Each changeset defines which packages should be published and whether the change should be a major/minor/patch release, as well as providing release notes that will be added to the changelog upon release.

To create a new changeset, run `pnpm changeset`. This will run the Changesets CLI, prompting you for details about the change. You’ll be able to edit the file after it’s created — don’t worry about getting everything perfect up front.

Even though you can technically use any markdown formatting you like, headings should be avoided since each changeset will ultimately be nested within a bullet list. Instead, bold text should be used as section headings.

If your PR is making changes to an area that already has a changeset (e.g. there’s an existing changeset covering theme API changes but you’re making further changes to the same API), you should update the existing changeset in your PR rather than creating a new one.

### Releasing to npm

The first time a PR with a changeset is merged after a release, a new PR will automatically be created called `Version Packages`. Any subsequent PRs with changesets will automatically update this existing version packages PR. Merging this PR triggers the release process by publishing to npm and cleaning up the changeset files.

### Creating a snapshot release

If a PR has changesets, you can create a [snapshot release](https://github.com/changesets/changesets/blob/main/docs/snapshot-releases.md) by [manually dispatching](https://github.com/fedimint/fedimint-sdk/actions/workflows/snapshot.yml) the Snapshot workflow. This publishes a tagged version to npm with the PR branch name and timestamp.

## 9. Updating dependencies

Use [Taze](https://github.com/antfu/taze) by running:

```bash
pnpm deps       # prints outdated deps
pnpm deps patch # print outdated deps with new patch versions
pnpm deps -w    # updates deps (best done with clean working tree)
```

<!-- [Socket](https://socket.dev) checks pull requests for vulnerabilities when new dependencies and versions are added, but you should also be vigilant! When updating dependencies, you should check release notes and source code as well as lock versions when possible. -->
