# Changelog

Release notes for the `fedimint-sdk` crate on crates.io, newest first. Each section also
becomes that version's GitHub Release.

The version bump pull request drafts the next section from the commits since the last
release. Rewrite it there for users, grouped under Added, Changed, Fixed and Breaking as
they apply, and remove the draft comment. The release refuses a version whose section is
missing or still a draft. See "Making a release" in [RELEASING.md](../../RELEASING.md).

## 0.1.0-beta.1

The first release of the Rust SDK, for testing. The API can still change before 0.1.0.

### Added

- `Sdk::builder()` starts an `SdkBuilder`, whose `build()` returns an `Sdk` over a `Storage`
  and a `Mnemonic`. Without a mnemonic it uses the seed already in the storage, or generates
  one into an empty storage.
- `Sdk::preview(&invite)` shows a federation before joining it, and `Sdk::join(&invite)` joins
  it. `Sdk::recover(&invite)` joins one and rescans its history for what the seed owns. It
  returns a `Recovery`: the federation, with an operation that reports the rescan's progress.
- A `Federation` has `balance()`, `capabilities()`, `activity()`, `meta()` and `operation(id)`,
  plus the `ecash()`, `lightning()` and `onchain()` facades, each `None` when the federation
  has no such module. Each facade has `quote`, which prices a send, `send` and `receive`. A
  send, and an ecash or Lightning receive, starts an operation to follow with `state()`,
  `updates()` and `await_final()`, returned on its own or with the notes or invoice to hand to
  the other side. An on-chain `receive()` returns an address only. A payment to it becomes an
  operation once the wallet finds it: it is then listed in `activity()` and handed out by the
  subscription that `deposits()` opens.
- Every fallible call returns an `Error`, with a stable `ErrorCode` to branch on, a message
  for people, and structured detail where there is any.
- Invite codes, invoices, addresses, ecash notes and ids are opaque types that parse with
  `FromStr` and print with `Display`.
- Builds for Linux, macOS, Android, iOS and `wasm32-unknown-unknown`. A `Storage` says where
  the data lives: `Storage::at(path)` is a directory on disk, on native targets,
  `Storage::in_browser(name)` is the browser's origin private file system, on wasm, and
  `Storage::in_memory()` keeps it in memory only, on both.
- The `uniffi` feature adds the UniFFI exports that the Kotlin, Swift and JavaScript SDKs are
  generated from.
- Built against fedimint 0.13.0-beta.1.
