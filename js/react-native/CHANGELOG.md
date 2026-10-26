# Changelog

Release notes for the React Native packages, `@fedimint/react-native` and
`@fedimint/react-native-bindings` on npm, newest first. Each section also becomes that
version's GitHub Release.

The version bump pull request writes the next section from the commits since the last
release, grouped under Breaking, Added, Changed and Fixed, and the section can be edited
in that pull request. The release refuses a version that has no section. See "Release
notes" in [RELEASING.md](../../RELEASING.md#release-notes).

## 0.1.0-beta.2

### Added

- `useBalance(federation)` and `useOperation(operation)`, React hooks that keep a component up to
  date with a federation's balance and an operation's state. Each takes an optional `onError`.
- `Exception.toString()` and `Exception.toDebugString()` give the error's code and reason, and the
  `message` of a thrown error includes them.

### Changed

- Built against fedimint 0.13.0-beta.1, where 0.1.0-beta.1 was built against an unreleased
  fedimint.
- `Meta.get(key)` and `Meta.all()` also read the document the federation publishes at its
  `meta_override_url`. A key takes its value from the consensus metadata first, then from that
  document, then from the configuration.
- The Android libraries are about half the size, and the iOS library no longer exports the Rust
  symbols in it to the app.

### Fixed

- `Notes.parse` accepts a v2 ecash token that is wrapped across lines or in another letter case,
  and an invoice may start with `lightning:`.

## 0.1.0-beta.1

The first release of the React Native SDK, for testing. The API can still change before 0.1.0.

The `0.0.0` and `0.0.0-*` versions of both packages on npm are an earlier API, and `latest` still
points at `0.0.0`: install this release by its version or with the `beta` tag.

### Added

- `@fedimint/react-native`, the SDK, and `@fedimint/react-native-bindings`, the native module under
  it. An app installs both, at the same version: React Native only links native modules that the
  app's own `package.json` lists.
- `openSdk({ dataDir, mnemonic? })` opens the SDK over a directory of the app and returns
  `{ sdk, InviteCode, Notes, Mnemonic, close }`.
- `sdk.preview(invite)` shows a federation before joining it, and `sdk.join(invite)` joins it.
  `sdk.recover(invite)` joins one and rescans its history for what the seed owns. It returns the
  federation with an operation that reports the rescan's progress.
- A federation has `balance()`, `capabilities()`, `activity()`, `meta()` and `operation(id)`, plus
  the `ecash()`, `lightning()` and `onchain()` facades. Each facade has `quote`, which prices a
  send, `send` and `receive`. A send, and an ecash or Lightning receive, starts an operation to
  follow with `state()`, `updates()` and `awaitFinal()`, returned on its own or with the notes or
  invoice to hand to the other side. An on-chain `receive()` returns an address only. A payment to
  it becomes an operation once the wallet finds it: it is then listed in `activity()` and handed out
  by the subscription that `deposits()` opens.
- Errors as `Exception`, with a stable code from `code()`, a message from `reason()` and structured
  detail, where there is any, from `details()`.
- The native libraries are inside `@fedimint/react-native-bindings`, for Android (`arm64-v8a`,
  `x86_64`) and iOS (devices and simulators). Nothing is downloaded or built when the packages are
  installed. An Android app has to be built for those two ABIs only.
- React Native 0.78 and later. Expo SDK 52 and later through the config plugin, with a custom dev
  client: Expo Go is not supported.
