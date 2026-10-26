# Releasing

How the SDK is released, for whoever cuts a release. Three things are published from this
repository, all built from the Rust crate in `rust/fedimint-sdk`:

| Component      | Published as                                                                                                                                                                                   | Version in the tree                                  | Release tag                   | Release notes                                                      |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------- | ----------------------------- | ------------------------------------------------------------------ |
| `rust`         | the [`fedimint-sdk`](https://crates.io/crates/fedimint-sdk) crate on crates.io                                                                                                                 | `rust/fedimint-sdk/Cargo.toml`                       | `rust-sdk-v<version>`         | [`rust/fedimint-sdk/CHANGELOG.md`](rust/fedimint-sdk/CHANGELOG.md) |
| `android`      | [`org.fedimint:sdk`](https://central.sonatype.com/artifact/org.fedimint/sdk) on Maven Central                                                                                                  | `fedimintSdk` in `android/gradle/libs.versions.toml` | `android-sdk-v<version>`      | [`android/CHANGELOG.md`](android/CHANGELOG.md)                     |
| `react-native` | [`@fedimint/react-native`](https://www.npmjs.com/package/@fedimint/react-native) and [`@fedimint/react-native-bindings`](https://www.npmjs.com/package/@fedimint/react-native-bindings) on npm | both `package.json` files under `js/react-native`    | `react-native-sdk-v<version>` | [`js/react-native/CHANGELOG.md`](js/react-native/CHANGELOG.md)     |

The Swift SDK in `ios/` and the browser package in `js/web/web-sdk` are not published yet.

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
   version for the chosen components and writes their release notes: one `## <version>` section
   in each chosen component's `CHANGELOG.md`, made from the commits since that component's last
   release, as "Release notes" below describes. It refuses a bump that does not fit, such as
   `patch` in the middle of a beta, or a version that is already taken, before it writes anything.

2. Read the notes in the pull request, in each file. Under the list of the files, the pull
   request lists the commits that got no entry in each component's notes, 50 at most, and
   `scripts/sdk-changelog.sh left-out <component> <last release>` lists them all. Check the Added
   and Fixed lines for a change that breaks callers, which belongs under Breaking when its commit
   had no `!`, and add a line for anything users see among the commits left out. Edit the notes
   there for anything else they need, for example to explain how to adapt to a breaking change.
   This is optional.
3. A pull request opened by a workflow does not start other workflows, so close and reopen it
   to start its checks, and let all of them finish before you merge. They also build the native
   libraries that the release then takes from the cache. One of them, **SDK Release Scripts**,
   runs `scripts/sdk-release.sh check-tree` on every pull request that changes a version or a
   `CHANGELOG.md`.
4. Merge it. Merging starts the release: the **SDK Tag** workflow runs when a push to `main`
   changes a file that holds a version. For each component whose version the push changed from a
   released one, it tags the merge commit and starts that component's release workflow on the
   tag. A component that has never been released is not tagged (see "The first release of a
   component").
5. Follow the release workflows under Actions. Each one stops before it publishes and asks a
   reviewer of its environment to approve: check that the run is on the tag of the release
   you merged, then approve. The rest needs nothing by hand:

   | Component      | Workflow                 | It publishes                                                   |
   | -------------- | ------------------------ | -------------------------------------------------------------- |
   | `rust`         | Rust SDK Release         | the crate to crates.io, and the GitHub Release                 |
   | `android`      | Android SDK Release      | the release to Maven Central, and the GitHub Release           |
   | `react-native` | React Native SDK Release | both packages to npm, and the GitHub Release with the tarballs |

Each workflow first checks its tag, then builds and tests the tagged commit, and publishes only
if all of that passed. The header of each workflow file says what it does in detail.
[`android/README.md`](android/README.md) and
[`js/react-native/README.md`](js/react-native/README.md) say what is particular to those two.

### Release notes

The version bump writes the notes of each component from the subjects of the commits since that
component's last release that touched what it is built from, newest first. A subject is read as a
conventional commit, `<type>(<scope>)!: <summary>` with the scope and the `!` optional, and its
type decides where the commit goes:

- `### Breaking`: any type with a `!` before the colon, such as `feat(sdk)!: ...`.
- `### Added`: `feat`.
- `### Fixed`: `fix`.

A section has the headings that have entries, in the order Breaking, Added, Changed, Fixed.
`### Changed` is for the one line that is not a commit, described below. An entry under the other
three is a list item holding the subject without its `<type>(<scope>)!: ` prefix and otherwise as
it is, so the number of the pull request, which a squash merge appends to the subject as `(#123)`,
stays. Commits of any other type are left out: `chore` (which includes the `chore(release): ...`
commit of a version bump), `build`, `ci`, `docs`, `test`, `refactor` and so on. Subjects that are
not in this form are left out too.

A commit that touched what a component is built from can still be about something else. Its scope,
the text in the parentheses, says so: a commit whose scope is foreign to a component is left out of
that component's notes, whatever its type and with or without a `!`. A scope is foreign to a
component when it names the development environment or the repository's tooling, or a part of the
repository that the component does not ship:

- `flake`, `ci`, `workflows`, `tests`, `examples` and `release` are foreign to every component.
- So are `web`, `sdk-web` and `vite-core` (the web SDK) and `swift` (the Swift SDK), which none of
  them ships.
- `react-native`, `rn-bindings` and `js` (the JavaScript workspace) are foreign to the crate and to
  the Android SDK, but not to the React Native SDK.
- `ios` is foreign to the Android SDK only. The React Native packages ship for iOS, and the crate
  runs on it.
- `android` is foreign to none of them, because the crate's Android code is in `rust/fedimint-sdk`,
  which all three ship.

`component_foreign_scopes` in [`scripts/sdk-components.sh`](scripts/sdk-components.sh) has the
lists, and `component_paths` in the same file what each component is built from. The scope is
compared whole, so `sdk-web` is not `web`. A commit with no scope, or with any other scope, is
listed as its type says, so a scope that is in none of the lists puts a commit in the notes rather
than losing it.

A revert is never listed, and it takes away the entry of the commit it reverts. Its subject is
`Revert "<subject>"`. `git revert` and GitHub's Revert button write there the subject of the
commit they revert, number included, but GitHub titles the revert's pull request
`Revert "<title>"`, without the number, and the title is what lands when that pull request has
several commits. So among the commits since the component's last release, a revert takes away the
entry of one commit, the one whose subject is `<subject>` or `<subject> (#<number>)`, and a commit
and its revert leave nothing in the notes. A revert that is itself reverted takes away nothing, so
the commit stays in the notes. The revert of a revert is written `Revert "Revert "<subject>""`, or,
by `git revert` 2.43 and later, `Reapply "<subject>"` followed by the numbers the revert had, to
which a squash merge may add one more. A reapply is never listed either. A revert of a commit that
an earlier release listed takes away nothing, because the notes of that release are written
already: if users need to hear about it, write a line by hand in the version bump pull request.

The crate's `Cargo.lock`, the one in `rust/fedimint-sdk`, says which fedimint every component is
built against. The bump reads its first `fedimint-core` package at the component's last release and
at the commit it runs on, and describes each as `fedimint <version>` when the package comes from
crates.io, or as `an unreleased fedimint` when it is a git revision. When the two differ, the notes
get `### Changed` with one line:
`Built against <new>, where <previous version> was built against <old>.` When both are git
revisions, and not the same one, the line is
`Built against another unreleased fedimint than <previous version>.` There is no such line for the
first release of a component, or when the lockfile or its `fedimint-core` package is missing at
either commit.

The version bump pull request lists, in a collapsed block for each component that has been
released before, the commits that got no entry in its notes: those of a type that is not listed,
subjects that are not in the form, commits with a foreign scope, reverts and reapplies, and the
commits that a revert took away. The block shows 50 commits at most, and
`scripts/sdk-changelog.sh left-out <component> <last release>` lists them all. Look through the
list for anything users see, and add a line for it by hand.

Two cases give a section of a single line:

- `No changes for users since <previous version>.` when no commit has an entry and the fedimint is
  the same;
- `The first release of the <title>.` for a component that has never been released, such as
  `The first release of the Android SDK.`

Pull requests are squash-merged with GitHub's "Default message" (see "What the releases rely on").
A pull request lands on `main` as one commit. Its subject is the subject of the pull request's
only commit, or the pull request's title when it has several, with ` (#<number>)` appended either
way, and that subject is what the notes read. A commit or a title that changes what users see is
therefore `feat(<scope>): ...` or `fix(<scope>): ...`, in words users understand, and one that
breaks callers marks its type with `!`, as in `feat(sdk)!: ...`. For a pull request with one
commit, a `!` or a better summary that review asks for goes into the commit, which is reworded,
and not only into the title, because the title is not what lands. Write both in this form.

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
  - A check refused a version. Nothing is tagged then, for any component. Fix it in a pull
    request, then run SDK Tag by hand on `main` (Actions → SDK Tag → Run workflow, which skips
    any other branch) for the same components. A manual run tags the components that have no tag
    for their version yet. The pull request that fixes it changes no version, so it does not
    start the workflow by itself.
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
  the next point). For the Android release, a re-run of the `publish` job for a version that
  Central published fails and changes nothing, because Central refuses a version it has. A re-run
  after a failed validation uploads again, as nothing was published. If the job ran out of time
  waiting for Central, the version may be out already: read "Releases" in
  [`android/README.md`](android/README.md) first.
  [`js/react-native/README.md`](js/react-native/README.md) says why re-running all jobs is
  wrong for the npm packages. Finish a release before the next one goes out: once any component
  has a newer release, the first job refuses the older tag, so that release can then only be
  finished by re-running its failed jobs, which leaves the first job out.
- **A release workflow refuses its tag,** or a release turns out bad. The first job of a release
  holds the tag to the release rules, which the header of
  [`scripts/sdk-release.sh`](scripts/sdk-release.sh) lists. The tag must be one of the
  component's own, name the version in the tree and be on `main`. The version must be newer than
  the earlier releases of every component, not only this one's, and than the versions on the
  registry this component is published to, if it has one. It must not be tagged on another commit
  by another component, and its notes must have a section for it. A tag pushed by hand starts a
  release too, under the same rules. A tag cannot be moved or deleted and no registry takes a
  version twice, so the fix is the next version: bump again, for the components concerned.
- **A version bump pull request is merged after another one.** Keep one version bump pull
  request open at a time. One that was opened from the same version is no longer the next version
  of the line once the first is merged: after `react-native` went from `0.1.0-beta.1` to `0.1.0`,
  the pull request that takes `android` to `0.1.0-beta.2` is behind the line. If it is merged all
  the same, the tag workflow refuses to tag a version that is lower than a release that is out,
  and nothing is released. Bump again for those components, which gives them the next version of
  the line, and move the notes of the version that was not released into the section of the new
  one: the bump writes that section above the old one and leaves the old one in place. The merge
  of this second bump changes a version that was never released, so run SDK Tag by hand for the
  components right after it (see "The first release of a component").
- **A version bump whose release went out was reverted.** Do not revert such a bump: the tree
  then says an older version than the release tags. The pull request check accepts that, as the
  older version has its tag, but the next bump of the same kind is refused because its version is
  already released: with the bump to `0.1.0-beta.2` reverted, `bump all beta` stops with "rust
  tags: 0.1.0-beta.2 is already released; bump the version first", and `check` refuses the
  version in the tree as lower than the latest release. Restore the versions and the notes in a
  pull request, that is, revert the revert.
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
  its environment, and the environment's required reviewers approve that job before it starts.
  Each has "Prevent self-review" on, so that whoever started a release run, for example by
  pushing a tag by hand, cannot approve it, and more than one reviewer for that reason. A run
  that the SDK Tag workflow started was started by GitHub Actions, so every reviewer can approve
  it. A job that names an environment which does not exist creates it without any rule, so they
  have to exist first.
- **Tag rulesets** (Settings → Rules → Rulesets) for `rust-sdk-v*`, `android-sdk-v*` and
  `react-native-sdk-v*`, which restrict updates and deletions and block force pushes, so that a
  release tag never moves. None may restrict creating a tag: the tag workflow creates the tags
  with its own token, and GitHub does not accept that on a ruleset's bypass list in an
  organization's repository.
- **"Allow GitHub Actions to create and approve pull requests"** (Settings → Actions →
  General), or the version bump cannot open its pull request.
- **Squash merges with the "Default message"** (Settings → General → Pull Requests → Allow squash
  merging → Default commit message, which the API calls `squash_merge_commit_title` and sets to
  `COMMIT_OR_PR_TITLE`). The generated release notes read the subjects that this setting produces:
  the subject of a pull request's only commit, or its title when it has several, with
  ` (#<number>)` appended. "Release notes" above is written for this setting: with another
  choice, the title of a pull request with one commit would land instead of the commit's subject,
  and review would fix the title rather than reword the commit.
- **Self-hosted runners** with the labels `self-hosted, linux, x64` and
  `self-hosted, macOS, ARM64, device-tests`. The Android release builds its native libraries on
  the first and runs the example app on an emulator on the second, and the React Native release
  builds its Android libraries on the first. A release waits in the queue while they are away.

The required reviewers are what decides whether something is published. Creating a release
tag is open to everybody who can push, and a workflow runs as its file is in the tagged
commit, so the checks in the release workflows stop mistakes, not misuse. Whoever approves a
publishing job checks that it is the release that was merged: its tag, and that the tagged
commit is on `main`.
