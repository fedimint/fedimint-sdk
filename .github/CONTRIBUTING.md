# Contributing

The development environment comes from [Nix](https://nixos.org). Install it, for example with the
[Determinate Nix Installer](https://github.com/DeterminateSystems/nix-installer), and run
`nix develop` in a checkout. That shell has Node.js, pnpm and `just`, and its first start takes a
while, because it downloads everything it needs. The Android and Apple toolchains and the local
federation the tests run against are in shells of their own, such as `nix develop .#android`.

The [`Justfile`](../Justfile) holds the build and test recipes, and `just --list` shows them.
These READMEs describe the parts of the repository:

- [`rust/fedimint-sdk`](../rust/fedimint-sdk): the SDK crate.
- [`android`](../android) and [`ios`](../ios): the Android and iOS SDKs.
- [`js/react-native`](../js/react-native): the React Native packages.
- [`js/web`](../js/web): the browser SDK.
- [`js/android/integration-tests`](../js/android/integration-tests): the tests that drive the
  Android example app on a device.

The JavaScript packages are a pnpm workspace rooted at `js/`. Run `pnpm --dir js install` once,
and `pnpm --dir js lint:fix` before committing: it formats the repository with prettier, which
continuous integration checks.

Pull requests are squash-merged: a pull request lands on `main` as one commit, whose subject is the
subject of the pull request's only commit, or the pull request's title when it has several, with
` (#<number>)` appended. Write the commit subjects and the title as `<type>(<scope>): <summary>`.
The `feat` and `fix` subjects become the release notes of the components the change reaches (see
"Release notes" in [`RELEASING.md`](../RELEASING.md#release-notes)), so write them for the people
who use the SDK. A change that breaks callers marks its type with `!`, as in `feat(sdk)!: ...`.
When a pull request has one commit, that commit's subject is what the notes read, so a `!` or a
better summary that review asks for goes into the commit (reword it), not only into the title. Scope
a subject by what the change is about: the notes of a component leave out the changes scoped to a
platform it does not ship, such as `ios` for the Android SDK, and to the development environment,
such as `flake` or `ci`. [`scripts/sdk-components.sh`](../scripts/sdk-components.sh) lists the
scopes that each component leaves out.

Please enable "Allow edits from maintainers" on a pull request, so that the core team can update
it if necessary.
