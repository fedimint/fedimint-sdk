# Releasing

How the SDK is released, for whoever cuts a release. Three things are published from this
repository, all built from the Rust crate in `rust/fedimint-sdk`:

| Component      | Published as                                                                                                                                                                                   | Version in the tree                                  | Release tag                   | Release notes                                                      |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- | ----------------------------- | ------------------------------------------------------------------ |
| `rust`         | the [`fedimint-sdk`](https://crates.io/crates/fedimint-sdk) crate on crates.io                                                                                                                 | `rust/fedimint-sdk/Cargo.toml`                       | `rust-sdk-v<version>`         | [`rust/fedimint-sdk/CHANGELOG.md`](rust/fedimint-sdk/CHANGELOG.md) |
| `android`      | [`org.fedimint:sdk`](https://central.sonatype.com/artifact/org.fedimint/sdk) on Maven Central                                                                                                  | `fedimintSdk` in `android/gradle/libs.versions.toml` | `android-sdk-v<version>`      | [`android/CHANGELOG.md`](android/CHANGELOG.md)                     |
| `react-native` | [`@fedimint/react-native`](https://www.npmjs.com/package/@fedimint/react-native) and [`@fedimint/react-native-bindings`](https://www.npmjs.com/package/@fedimint/react-native-bindings) on npm | both `package.json` files under `js/react-native`    | `react-native-sdk-v<version>` | [`js/react-native/CHANGELOG.md`](js/react-native/CHANGELOG.md)     |

The Swift SDK in `ios/` and the browser package in `js/web/sdk-web` are not published yet.

## Versions

A version is `X.Y.Z-beta.N`, a beta for testing, or `X.Y.Z`, a release. Nothing else is
accepted: no alpha, no rc. Versions follow [semantic versioning](https://semver.org) and
describe what a caller of the component sees. The Kotlin and JavaScript APIs are generated from
the Rust one, so a change to the Rust API usually counts for all three.

| Change since the last release                                                               | Before 1.0.0 | From 1.0.0 |
| ------------------------------------------------------------------------------------------- | ------------ | ---------- |
| Breaks callers: removes or renames a type, function, parameter or enum case, changes a type | `minor`      | `major`    |
| Adds to the API without breaking anything                                                   | `minor`      | `minor`    |
| Fixes behaviour, with no change to the API                                                  | `patch`      | `patch`    |

Going to 1.0.0 is a `major` bump, made on purpose once the API is stable. Before that, a
`minor` bump is how a breaking change is signalled, as semver allows for 0.x.

The version of a component is kept in the file the first table names, and the crate's also in
`rust/fedimint-sdk/Cargo.lock` and `ios/Sources/FedimintSdk/Version.swift`. Do not edit it by
hand: the version bump sets it (see "Making a release").

### One line of versions

The three components share one line of versions.

- A release takes the next version of the line and gives it to the components it publishes.
- The usual release publishes all three. They then carry the same version and are built from
  the same commit.
- A point release publishes only the bindings (the Android SDK and the React Native packages),
  or only one of them. Those get the next version of the line and the others keep theirs. So a
  component's versions can have gaps: after the Android SDK alone went from `0.2.0` to `0.2.1`,
  the next patch release of everything is `0.2.2`. The crate is only released with both
  bindings, as `all`.
- A version is used once. It is newer than every version released before it, of any component,
  and it is never given to two different commits.

`0.1.0-beta.1`, the first release of each component, is the exception to the rule that a version
is never given to two commits: the three releases are not on one commit.

A version moves like this:

```
0.1.0 --minor, as beta--> 0.2.0-beta.1 --beta--> 0.2.0-beta.2 --release--> 0.2.0
0.2.0 --patch-------------------------------------------------------------> 0.2.1
```

A beta is always either continued (`beta`) or released (`release`). There is no `patch`,
`minor` or `major` in the middle of one.

## Making a release

1. Run the **SDK Version Bump** workflow on `main` (Actions → SDK Version Bump → Run
   workflow). It refuses to run on any other branch. Choose what to release and how to bump:

   | `components`              | Publishes                                                    |
   | ------------------------- | ------------------------------------------------------------ |
   | `all`                     | the crate, the Android SDK and the React Native packages     |
   | `bindings`                | the Android SDK and the React Native packages, not the crate |
   | `android`, `react-native` | that binding alone                                           |

   | `bump`                    | From      | To                                                   |
   | ------------------------- | --------- | ---------------------------------------------------- |
   | `patch`, `minor`, `major` | a release | the next version, at `-beta.1` when `beta` is ticked |
   | `beta`                    | a beta    | the next beta                                        |
   | `release`                 | a beta    | that version, released                               |

   The `beta` box is ticked by default and only matters for `patch`, `minor` and `major`.

   The workflow opens a pull request from the branch `sdk/version-<version>`. It sets the new
   version for the chosen components and drafts their release notes: one `## <version>` section
   in each chosen component's `CHANGELOG.md`, listing the commits since that component's last
   release that touched what it is built from. It refuses a bump that does not fit, such as
   `patch` in the middle of a beta, or a version that is already taken, before it writes anything.

2. Rewrite the drafted notes for users, in each file, in that component's own terms: what was
   added, changed or fixed, and anything that breaks callers. Delete the draft comments. The
   pull request check (step 3) refuses a draft, and so does the release.
3. A pull request opened by a workflow does not start other workflows, so close and reopen it
   to start its checks, and let all of them finish before you merge. They also build the native
   libraries that the release then takes from the cache. One of them, **SDK Release Scripts**,
   runs `scripts/sdk-release.sh check-tree` on every pull request that changes a version or a
   `CHANGELOG.md`, so a version bump whose notes are still drafts fails there, before it is
   merged.
4. Merge it. Merging starts the release: the **SDK Tag** workflow runs when a push to `main`
   changes a file that holds a version. For each component whose version the push changed from a
   released one, it tags the merge commit and starts that component's release workflow on the
   tag. A component that has never been released is not tagged (see "The first release of a
   component").
5. Follow the release workflows under Actions. Each one stops before it publishes and asks a
   reviewer of its environment to approve: check that the run is on the tag of the release
   you merged, then approve. Then finish what is left by hand:

   | Component      | Workflow                 | It publishes                                                                    | Left to do by hand                                                                                                           |
   | -------------- | ------------------------ | ------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
   | `rust`         | Rust SDK Release         | the crate to crates.io, and the GitHub Release                                  | nothing                                                                                                                      |
   | `android`      | Android SDK Release      | an upload to Maven Central that is not yet released, and a draft GitHub Release | On central.sonatype.com, wait for the deployment to show VALIDATED and press Publish. Then publish the draft GitHub Release. |
   | `react-native` | React Native SDK Release | both packages to npm, and the GitHub Release with the tarballs                  | nothing                                                                                                                      |

Each workflow first checks its tag, then builds and tests the tagged commit, and publishes only
if all of that passed. The header of each workflow file says what it does in detail.
[`android/README.md`](android/README.md) and
[`js/react-native/README.md`](js/react-native/README.md) say what is particular to those two.

### The first release of a component

The merge tags a component only if the version it changed from was released. A component that
has never been released is left out, so that adding one to the tree releases nothing by surprise.
The pull request lists such components, and the log of the tag job names each one it left out.

Start the first release of such a component by running **SDK Tag** by hand (Actions → SDK Tag →
Run workflow, on `main`, with `components` set to the component or to a group that holds it)
right after the version bump is merged, before anything else is merged. Components released
together are tagged on one commit, and a version is never tagged on two commits. A commit merged
in between would put the tag on a later commit than the tags the merge made for the other
components. The release rules would refuse it, and the component would need the next version.

## When something goes wrong

- **The version bump fails.** Its log says why. If it stops after it pushed its branch
  `sdk/version-<version>`, for example because the setting that lets Actions open pull requests
  is off, delete the branch, and close its pull request if there is one, before running the
  workflow again.
- **The tag workflow fails.** Its error says what to do. Once the tags are pushed, the summary
  of the run lists them, the releases it started and the ones it did not.
  - A check refused a version, typically notes that were merged as a draft. Nothing is tagged
    then, for any component. Fix it in a pull request, then run SDK Tag by hand on `main`
    (Actions → SDK Tag → Run workflow, which skips any other branch) for the same components. A
    manual run tags the components that have no tag for their version yet. The pull request that
    fixes it changes no version, so it does not start the workflow by itself.
  - The workflow could not decide what to release. Nothing is tagged then either. Once the cause
    is fixed, run SDK Tag by hand on `main` in the same way. A release whose automatic run was
    skipped or lost is started like this too.
  - The tags could not be pushed. Nothing is tagged then either. Re-run the job once the cause
    is fixed.
  - A release workflow could not be started. The tags exist, so re-running the job does not
    start it. Start the release workflow by hand on its tag: Actions → the workflow → Run
    workflow, choose the tag (not a branch) under "Use workflow from", and tick `publish`. The
    summary names each workflow left to start this way.
- **A release workflow fails half way.** Re-run its failed jobs, not all jobs (Actions → the
  failed run → Re-run failed jobs). A crate that crates.io already has from the tagged commit is
  not published again, and an npm package that is already there as the tarball this run built is
  skipped, so both releases go on from where they stopped. The GitHub Release is created or
  updated again. A version that a registry has from another build stops the release instead (see
  the next point). The Android release does not look at Maven Central, so re-running its
  `publish` job uploads again: read "Releases" in [`android/README.md`](android/README.md)
  first. [`js/react-native/README.md`](js/react-native/README.md) says why re-running all jobs is
  wrong for the npm packages.
- **A release workflow refuses its tag,** or a release turns out bad. The first job of a release
  holds the tag to the release rules, which the header of
  [`scripts/sdk-release.sh`](scripts/sdk-release.sh) lists. The tag must be one of the
  component's own, name the version in the tree and be on `main`. The version must be newer than
  the component's earlier releases and not tagged on another commit by another component, and
  its notes must be finished. A tag pushed by hand starts a release too, under the same rules. A
  tag cannot be moved or deleted and no registry takes a version twice, so the fix is the next
  version: bump again, for the components concerned.
- **One component's release fails for good while the others went out.** The others stay
  released. Fix the cause and make a point release of the failed binding, or of `bindings` if both
  failed: it gets the next version of the line. The crate has no point release of its own, so a
  failed crate release goes out again with the next release of everything.

## Trying a release without releasing

Each release workflow can be run by hand on any branch (Actions → the workflow → Run workflow,
with `publish` unticked, which is the default). That is a dry run: it builds and tests what a
release would, and publishes nothing, tags nothing and creates no GitHub Release.

| Workflow                 | The dry run does                                                                                                                                                      | Waits for an environment |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ |
| Rust SDK Release         | the version check, all of the crate's continuous integration, and `cargo package`                                                                                     | no                       |
| Android SDK Release      | the version check and the whole Android build and test chain, then it signs the artifacts into a local Maven repository, kept as the `maven-central-dry-run` artifact | yes, `maven-central`     |
| React Native SDK Release | the version check, the native builds, the tarballs and their tests, then `npm publish --dry-run`                                                                      | no                       |

On a branch, a dry run takes the version from the tree and checks it as a release would, but a
version that could not be released only gives a warning, and the run goes on. So on the branch of
a version bump pull request, a dry run shows whether that release would go through: a warning
means it would not. Run on a release tag, a dry run holds the tag to the release rules like a
release does, and fails if the tag is refused.

The Android dry run signs the artifacts with the key held by the `maven-central` environment, so
its `publish` job (shown as "Dry run") waits for that environment's required reviewers like a
release does. Approving it hands the environment's secrets to the workflow file as it is on that
branch. The reviewer therefore looks at what the branch changes under `.github/workflows` and in
the scripts the job runs (the Gradle build under `android/`) before approving. The Rust and React
Native dry runs reach no environment: only their `publish` jobs name one, and those run on a tag
only, for a tag push or a run with `publish` ticked.

The scripts that hold the rules work in a shell too. While the line is at `0.1.0-beta.1`, for
example:

```sh
scripts/sdk-version.sh current                             # the version of the line: 0.1.0-beta.1
scripts/sdk-version.sh current android                     # one component's version: 0.1.0-beta.1
scripts/sdk-version.sh next release                        # what releasing a beta gives: 0.1.0
scripts/sdk-release.sh check-tree                          # is the committed state releasable?
scripts/sdk-changelog.sh release-notes rust 0.1.0-beta.1   # the notes of a GitHub Release
```

`next` takes the arguments of the bump. From a beta it gives `beta` and `release`, from a release
`patch`, `minor` and `major` (with `--beta` to start a beta), and it refuses the others.
`scripts/test-sdk-release-scripts.sh` tests the scripts and needs no build.

## What the releases rely on

Settings outside this repository's files. All of them have to be in place before a release.

- **crates.io trusted publishing.** The `fedimint-sdk` crate names this repository, the workflow
  file `rust-sdk-release.yaml` and the environment `crates-io` as its trusted publisher
  (crates.io → the crate → Settings → Trusted Publishing). No token is stored anywhere. crates.io
  only lets an existing crate set up trusted publishing, so the crate's first version,
  `0.1.0-beta.1`, was published by hand, and its tag and GitHub Release were made by hand as
  well, because the release rules refuse a version that another component tagged on another
  commit.
- **npm trusted publishing.** Each of the two packages names this repository, the workflow file
  `react-native-sdk-release.yaml` and the environment `npm` as a trusted publisher, with
  `npm publish` among the allowed actions.
- **Maven Central credentials.** A Central Portal user token (`MAVEN_CENTRAL_USERNAME` and
  `MAVEN_CENTRAL_PASSWORD`) and the signing key with its passphrase (`MAVEN_SIGNING_KEY` and
  `MAVEN_SIGNING_KEY_PASSWORD`), as secrets of the `maven-central` environment and nowhere else.
  The key's public half is on a keyserver that Central checks, such as keys.openpgp.org.
- **Three environments with required reviewers** (Settings → Environments): `crates-io`,
  limited to tags matching `rust-sdk-v*`; `npm`, limited to tags matching
  `react-native-sdk-v*`; and `maven-central`, which is not limited to tags because the Android
  dry run and the Android SDK Snapshot workflow use it from branches. Each release publishes in
  its environment, and the environment's required reviewers approve that job before it starts. A
  job that names an environment which does not exist creates it without any rule, so they have
  to exist first.
- **Tag rulesets** (Settings → Rules → Rulesets) for `rust-sdk-v*`, `android-sdk-v*` and
  `react-native-sdk-v*`, which restrict updates and deletions and block force pushes, so that a
  release tag never moves. None may restrict creating a tag: the tag workflow creates the tags
  with its own token, and GitHub does not accept that on a ruleset's bypass list in an
  organization's repository.
- **"Allow GitHub Actions to create and approve pull requests"** (Settings → Actions →
  General), or the version bump cannot open its pull request.
- **Self-hosted runners** with the labels `self-hosted, linux, x64` and
  `self-hosted, macOS, ARM64, device-tests`. The Android release builds its native libraries on
  the first and runs the example app on an emulator on the second, and the React Native release
  builds its Android libraries on the first. A release waits in the queue while they are away.

The required reviewers are what decides whether something is published. Creating a release
tag is open to everybody who can push, and a workflow runs as its file is in the tagged
commit, so the checks in the release workflows stop mistakes, not misuse. Whoever approves a
publishing job checks that it is the release that was merged: its tag, and that the tagged
commit is on `main`. For the Android SDK, pressing Publish on central.sonatype.com is a second
decision after that.
