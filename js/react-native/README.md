# React Native packages

Two npm packages live here. They are built from one commit and always released together:

- [`react-native-bindings`](react-native-bindings) is `@fedimint/react-native-bindings`, the native
  module: the bindings generated from `rust/fedimint-sdk`, and the native libraries they call.
- [`react-native`](react-native) is `@fedimint/react-native`, the API apps use, on top of the
  bindings.

Each package's own README is its user documentation and goes to npm with it. This file is about
working on the packages in this repository.

## Bindings and native libraries

Run `just generate-sdk-rn-bindings` after any change to the crate's UniFFI surface, then commit
the result. CI regenerates on every PR and fails the build if the working tree differs
afterwards, so a stale commit is caught before merge.

The npm package `uniffi-bindgen-react-native` is a direct dependency of the bindings, but only
for what the turbo-module compiles against: its C++ runtime headers (`cpp/includes`, read by
CMake and Xcode) and its CocoaPod. Its own `ubrn` CLI generates for UniFFI 0.31, one minor
version behind the crate, so it is never run; `generate-sdk-rn-bindings.sh` refuses a `ubrn` that
resolves under `node_modules` and the bindings keep no `ubrn:*` scripts.

The native libraries are build outputs and not in git.

Android: `just generate-sdk-rn-bindings` reads `rust/fedimint-sdk` built for Android through nix
(`.#fedimint-sdk-android-jni`) and copies the resulting `.so` files into
`react-native-bindings/android/src/main/jniLibs/<abi>/`.

iOS: `just build-rn-ios`, run on macOS with Xcode installed, cross-compiles the crate for the
configured targets and links the result into
`react-native-bindings/FedimintReactNativeBindingsFramework.xcframework`. A release assembles
that bundle from the libraries the Swift SDK is built from instead (see Packing locally).

## Publishing

### Versioning

The two packages share one version, which is on the SDK's shared line of versions:
[`RELEASING.md`](../../RELEASING.md#versions) has the rules and says which bump a change calls
for. A version is `X.Y.Z-beta.N` (a beta, published under the npm dist-tag `beta`) or `X.Y.Z` (a
release, published under `latest`), and nothing else.

The version is in the tree, in both `package.json` files, and the SDK Version Bump workflow sets
it. Both files stay `"private": true`, which keeps everything else in the repository that
publishes to npm away from these two packages. The tarballs a release packs carry the version and
no `private`.

`@fedimint/react-native` names `@fedimint/react-native-bindings` as a peer dependency at exactly
its own version. The generated API and the native library behind it only fit together when they
come from the same build.

While every release is a beta, the install commands in
[`react-native/README.md`](react-native/README.md) name the `beta` tag. They lose it with the
first `X.Y.Z`, which is what moves `latest`.

### Releases

[`react-native-sdk-release.yaml`](../../.github/workflows/react-native-sdk-release.yaml) does the
whole release. How a release is started is in [`RELEASING.md`](../../RELEASING.md). Pushing a
`react-native-sdk-v*` tag by hand starts one too, under the same checks. On the tagged commit the
workflow:

1. checks the tag against the rules of [`scripts/sdk-release.sh`](../../scripts/sdk-release.sh)
   (see below);
2. builds the native libraries, the same builds the Android and Swift SDKs ship, and assembles the
   Apple ones into the iOS bundle the podspec vendors;
3. regenerates the bindings from those libraries and fails if they differ from the committed ones;
4. packs both packages, with the native libraries inside, and checks the tarballs;
5. installs the two tarballs into a copy of the [bare example app](../examples/react-native), the
   way an app adds them, and builds it for Android and iOS. The Android app is also run on an
   emulator, where it opens the SDK and generates a mnemonic;
6. publishes both tarballs to npm, bindings first, once a reviewer of the `npm` environment has
   approved the job;
7. creates the GitHub Release from the version's section of [`CHANGELOG.md`](CHANGELOG.md), with
   both tarballs and their checksums attached.

A release is refused if the tag is not a `react-native-sdk-v*` tag that names the version the
packages have in the tree, if its commit is not on `main`, if its notes are missing or still a
draft, if another component has the same version tagged on another commit, or if the version is
not newer than every earlier release tag, of the React Native SDK or of another component, and
every version of either package on npm, snapshots included. The header of
[`scripts/sdk-release.sh`](../../scripts/sdk-release.sh) has the full rules.

A tag cannot be moved or deleted, and a version on npm cannot be published again. A release that
turns out wrong is fixed by releasing the next version.

If a release fails halfway, run its failed jobs again (Actions → the failed run → Re-run failed
jobs). They reuse the tarballs the run built and tested: a package that already made it to npm
is skipped, and the release continues from there. Re-run all jobs is not the way to finish a
release: it builds the packages again, and if a package that is already on npm comes out any
different, the release stops rather than publish a pair that was never tested together. A
release also stops when the dist-tag it publishes under, `beta` or `latest`, has moved on to a
newer version in the meantime, since finishing it would move the dist-tag back. In both cases,
release the next version.

To try the release without releasing, run the workflow by hand (Actions → React Native SDK
Release → Run workflow) on any branch, with `publish` unticked, which is the default. That is a
dry run: it takes the version from the tree and does what a release does up to and including the
tests of the tarballs, and then `npm publish --dry-run`. It never publishes, tags or creates a
release, and it reaches no environment.

### Packing locally

The same scripts run outside CI. Packing needs the Android libraries, the built packages and the
iOS xcframework in place:

```sh
just generate-sdk-rn-bindings
nix develop --accept-flake-config .#android -c pnpm --dir js run build:reactnative
just build-ios-lib-nix && just assemble-rn-ios-xcframework    # macOS with Xcode only
nix develop --accept-flake-config .#android -c \
  scripts/react-native-sdk-pack.sh 0.1.0-beta.2 /tmp/react-native-tarballs
```

The first argument of the pack script is the version to write into the tarballs. A release passes
the version in the tree, which `scripts/sdk-version.sh current react-native` prints.

The xcframework can only be assembled on macOS. On Linux, take it from a run of the release
workflow instead:

```sh
gh run download <run id> --name react-native-xcframework --dir \
  js/react-native/react-native-bindings/FedimintReactNativeBindingsFramework.xcframework
```

To build and run the example against the tarballs, as the release does:

```sh
nix develop --accept-flake-config .#rn-android-emulator
scripts/react-native-sdk-example.sh /tmp/react-native-app \
  /tmp/react-native-tarballs/fedimint-react-native-bindings-*.tgz \
  /tmp/react-native-tarballs/fedimint-react-native-[0-9]*.tgz
(cd /tmp/react-native-app/android && ENTRY_FILE=index.smoke.js ./gradlew :app:assembleRelease)
scripts/react-native-sdk-smoke.sh --boot \
  /tmp/react-native-app/android/app/build/outputs/apk/release/app-release.apk
```

`scripts/test-react-native-sdk-scripts.sh` tests how the packages are packed and published, and
`scripts/test-sdk-release-scripts.sh` the rules every release passes. Neither needs a build.

### What a release relies on

The release holds no npm credential. Three settings outside this repository's files have to be
in place before a release:

- **npm trusted publishing.** Each of the two packages on npmjs.com (Settings → Trusted
  Publisher) names this repository, the workflow file `react-native-sdk-release.yaml` and the
  environment `npm` as its publisher, with `npm publish` among the allowed actions. npm drops a
  trusted publisher that has not published within two days of being set up, so set it up right
  before the first release. Once a release has gone through, set both packages' publishing access
  to "Require two-factor authentication and disallow tokens": no token can publish them then.
  That leaves this workflow and a maintainer who publishes by hand, signed in with two-factor
  authentication.
- **The `npm` environment** (Settings → Environments), with required reviewers and with
  deployments limited to tags matching `react-native-sdk-v*`. It has "Prevent self-review" on, so
  that whoever started a release run, for example by pushing a tag by hand, cannot approve it,
  and more than one reviewer for that reason. A run that the SDK Tag workflow started was started
  by GitHub Actions, so every reviewer can approve it. The `publish` job is the only one that
  names the environment. A job that names an environment which does not exist creates it without
  any limit, so it has to exist first.
- **A tag ruleset** for `refs/tags/react-native-sdk-v*` (Settings → Rules → Rulesets) that
  restricts updates and deletions and blocks force pushes, with nobody on its bypass list. No
  ruleset restricts creating a release tag: the tag workflow creates the tags with the workflow's
  own token, which GitHub does not accept on a ruleset's bypass list in an organization's
  repository.

What decides whether something is published is the `npm` environment. Its required reviewers
approve every run of the publishing job, and they have to be configured: naming the environment
in the workflow adds none. Everybody who can push can create a release tag on any commit, and a
workflow runs as its file is in the tagged commit, so the workflow's own check stops mistakes, not
misuse.

Both packages also have versions on npm that this release did not make: `0.0.0` and a number of
`0.0.0-*` snapshots of an earlier API, with `latest` on `0.0.0`. An npm maintainer deprecates
them, so that installing one prints where to go:

```sh
npm deprecate "@fedimint/react-native@<=0.0.0" \
  "Install @fedimint/react-native@beta and @fedimint/react-native-bindings@beta instead"
npm deprecate "@fedimint/react-native-bindings@<=0.0.0" \
  "Install @fedimint/react-native@beta and @fedimint/react-native-bindings@beta instead"
```
