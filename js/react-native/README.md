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

The packages share one version, independent of `rust/fedimint-sdk`'s and of the other SDKs'. It
follows [semantic versioning](https://semver.org) and describes the JavaScript API. A version is
`X.Y.Z-beta.N` (a beta, published under the npm dist-tag `beta`) or `X.Y.Z` (a release, published
under `latest`), and nothing else.

The version is not in the tree. Both `package.json` files say `0.0.0` and `"private": true`, and
stay that way: the version of a release is the name of its git tag,
`react-native-sdk-v<version>`, and the release writes it into the packages it packs. `private`
keeps everything else in the repository that publishes to npm away from these two packages.

`@fedimint/react-native` names `@fedimint/react-native-bindings` as a peer dependency at exactly
its own version. The generated API and the native library behind it only fit together when they
come from the same build.

While every release is a beta, the install commands in the two packages' READMEs name the `beta`
tag. They lose it with the first `X.Y.Z`, which is what moves `latest`.

### Releases

[`react-native-sdk-release.yaml`](../../.github/workflows/react-native-sdk-release.yaml) does the
whole release when a `react-native-sdk-v*` tag is pushed. Nobody approves anything and nothing is
published by hand. On the tagged commit it:

1. checks the tag (see below);
2. builds the native libraries, the same builds the Android and Swift SDKs ship;
3. regenerates the bindings from those libraries and fails if they differ from the committed ones;
4. packs both packages, with the native libraries inside, and checks the tarballs;
5. installs the two tarballs into a copy of the [bare example app](../examples/react-native), the
   way an app adds them, and builds it for Android and iOS. The Android app is also run on an
   emulator, where it opens the SDK and generates a mnemonic;
6. publishes both tarballs to npm, bindings first;
7. creates the GitHub Release from the tag's message, with both tarballs attached.

The tag's message is the release notes. To release:

```sh
git fetch origin main --tags
scripts/react-native-sdk-release.sh notes > notes.md
# Rewrite notes.md for users: what was added, changed or fixed, and what breaks callers.
scripts/react-native-sdk-release.sh tag 0.1.0-beta.2 notes.md
git push origin react-native-sdk-v0.1.0-beta.2
```

`notes` lists the commits since the last release that touched what the packages are built from.
`tag` runs the same checks the release will, then creates the annotated tag at `origin/main`
(pass a commit as a third argument to tag an earlier one). Pushing the tag starts the release.
To tag by hand instead, use `git tag -a --cleanup=whitespace -F notes.md <tag> <commit>`:
without `--cleanup=whitespace`, git drops every line of the notes that starts with `#`, Markdown
headings included.

A release is refused if the tag is not annotated or has no message, if the tagged commit is not
on `main`, if the version is not a beta or a release, or if it is not newer than every earlier
`react-native-sdk-v*` tag and every version of either package on npm.

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
Release → Run workflow) on any branch, with a version. That is a dry run: it does what a release
does up to and including the tests of the tarballs, and then `npm publish --dry-run`. It never
publishes, tags or creates a release.

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

`scripts/test-react-native-sdk-scripts.sh` tests the release scripts themselves and needs no
build.

### What a release relies on

The release holds no npm credential. Three settings outside this repository's files make it
safe to release from a pushed tag, and all three have to be in place before the first one:

- **npm trusted publishing.** Each of the two packages on npmjs.com (Settings → Trusted
  Publisher) names this repository, the workflow file `react-native-sdk-release.yaml` and the
  environment `npm` as its publisher, with `npm publish` among the allowed actions. npm drops a
  trusted publisher that has not published within two days of being set up, so set it up right
  before the first release. Once a release has gone through, set both packages' publishing access
  to "Require two-factor authentication and disallow tokens": no token can publish them then.
  That leaves this workflow and a maintainer who publishes by hand, signed in with two-factor
  authentication.
- **The `npm` environment** (Settings → Environments), with no required reviewers and with
  deployments limited to tags matching `react-native-sdk-v*`. The `publish` job is the only one
  that names it. A job that names an environment which does not exist creates it without any
  limit, so it has to exist first.
- **Two tag rulesets** for `refs/tags/react-native-sdk-v*` (Settings → Rules → Rulesets): one
  that restricts creations, with the maintainers on its bypass list, and one that restricts
  updates and deletions and blocks force pushes, with nobody on its bypass list. They are
  separate because a bypass list covers every rule of its ruleset.

Both packages also have versions on npm that this release did not make: `0.0.0` and a number of
`0.0.0-*` snapshots of an earlier API, with `latest` on `0.0.0`. Once the first beta is out, an
npm maintainer deprecates them, so that installing one prints where to go:

```sh
npm deprecate "@fedimint/react-native@<=0.0.0" \
  "Install @fedimint/react-native@beta and @fedimint/react-native-bindings@beta instead"
npm deprecate "@fedimint/react-native-bindings@<=0.0.0" \
  "Install @fedimint/react-native@beta and @fedimint/react-native-bindings@beta instead"
```
