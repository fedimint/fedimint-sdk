# Changelog

Release notes for the Android SDK, `org.fedimint:sdk` on Maven Central,
newest first. Each section also becomes that version's GitHub Release.

The version bump pull request drafts the next section from the commits since
the last release. Rewrite it there for users, grouped under Added, Changed,
Fixed and Breaking as they apply, and remove the draft comment. The release
refuses a version whose section is missing or still a draft. See the
"Versioning" section of [README.md](README.md).

## 0.1.0-beta.1

The first release of the Android SDK, for testing. The API can still
change before 0.1.0.

### Added

- `createFedimintSdk(dataDir, seed?)` opens the SDK over an app-private
  directory. Pass a `Mnemonic` to restore a seed, or `null` to load the one
  already there, or to create one in an empty directory.
- `Sdk`: `exportMnemonic()`, `preview(invite)` to read a federation's config
  without joining, and `join(invite)`, which returns a `Federation`.
- `Federation`: `balance()`, `capabilities()`, `activity()`, `meta()` and
  `operation(id)`, plus the `ecash()`, `lightning()` and `onchain()` facades.
  Their `quote`, `send` and `receive` calls return operation handles to follow
  with `state()`, `updates()` and `awaitFinal()`.
- `Mnemonic` and `InviteCode` as opaque handles, so a seed or an invite code
  is never handed to Kotlin as a loggable string.
- Errors as `org.fedimint.sdk.Exception`, with an `ErrorCode` from `code()` and
  a message from `reason()`.
- Native libraries for `arm64-v8a` and `x86_64`, minimum SDK 28. The library
  declares the `INTERNET` and `ACCESS_NETWORK_STATE` permissions.
