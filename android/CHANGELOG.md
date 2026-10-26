# Changelog

Release notes for the Android SDK, `org.fedimint:sdk` on Maven Central,
newest first. Each section also becomes that version's GitHub Release.

The version bump pull request writes the next section from the commits since
the last release, grouped under Breaking, Added, Changed and Fixed, and the
section can be edited in that pull request. The release refuses a version that
has no section. See "Release notes" in
[RELEASING.md](../RELEASING.md#release-notes).

## 0.1.0-beta.2

### Breaking

- `Onchain.receive()` returns the address alone. A payment to it becomes an
  operation once the wallet finds it, starting in the `Confirmed` state.
  `Onchain.deposits()` opens a subscription whose `next()` returns each
  deposit found after it was opened, so open it before showing the address.
  `OnchainReceiveHandle` and the `WaitingForTransaction` and
  `WaitingForConfirmation` states are gone, and `activity()` no longer lists
  an address before it is paid.
- `RecoveryState.Running` is a class with a `progress` property: a
  `RecoveryProgress` with how much of the rescan is `complete` out of its
  `total`, or `null` until that is known. Check for it with `is`.
- On-chain receives that 0.1.0-beta.1 recorded are not migrated. An address
  it handed out that was never paid is listed in `activity()` with an unknown
  status, and `forgetFederation` refuses to forget its federation with
  `PENDING_OPERATIONS`.

### Added

- `Exception.toString()` gives the error's code and reason.

### Changed

- Built against fedimint 0.13.0-beta.1, where 0.1.0-beta.1 was built against
  an unreleased fedimint.
- `Meta.get(key)` and `Meta.all()` also read the document the federation
  publishes at its `meta_override_url`. A key takes its value from the
  consensus metadata first, then from that document, then from the
  configuration.
- More failures report an `ErrorCode` of their own. A Lightning v1 send
  reports `GATEWAY_UNAVAILABLE`, `FEDERATION_UNREACHABLE`,
  `AMOUNTLESS_INVOICE`, `INVALID_INPUT` or `STORAGE` where it reported
  `INTERNAL`. Joining, reopening or recovering a federation reports `STORAGE`
  only for a problem with the stored data, and `FEDERATION_UNREACHABLE` or
  `INTERNAL` otherwise.
- A DNS lookup made to reach the guardians goes to the public DNS servers of
  Cloudflare, Google and Quad9 when every DNS server of the device refuses it
  or fails it with a server error.
- On a federation with wallet v2, `Onchain.receive()` fails with `TIMEOUT`
  when the wallet has not derived an address within 120 seconds.
- The native libraries are about half the size.

### Fixed

- A recovery reaches `Done` only once the wallet holds everything the rescan
  found, so the balance is complete by then.
- A recovery no longer keeps the data directory locked after every handle to
  the SDK is released without `shutdown()`.
- A Lightning v2 receive picks a gateway that accepts incoming payments,
  rather than the first one that answers.
- A Lightning v2 send that fails with `INSUFFICIENT_BALANCE` gives the amount
  it needed and the balance in its details.
- `Notes.parse` accepts a v2 ecash token that is wrapped across lines or in
  another letter case, and an invoice may start with `lightning:`.
- Opening a data directory that another process is about to release waits
  for it briefly, instead of failing with `STORAGE_IN_USE` at once.

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
