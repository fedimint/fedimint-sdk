# Fedimint iOS SDK

A Swift SDK generated with [UniFFI](https://mozilla.github.io/uniffi-rs/) from
[`fedimint-sdk`](../rust/fedimint-sdk)'s `uniffi` feature — the same feature the
[Android SDK](../android) is generated from.

There is **no hand-written Swift here** (one build-shim file aside, which says so
itself). The `#[uniffi::export]` blocks in
[`fedimint-sdk`](../rust/fedimint-sdk) export that crate's real methods as-is and
hand out its own types — `Sdk`, `Federation`, `Mnemonic`, `InviteCode`, `Notes`,
`Ecash`, `Lightning`, `Onchain`, the seven `*Operation` handles, `SdkError`,
`ErrorCode` — so this SDK is a view of that API rather than a copy of it that
could drift.

## Installing

Add the package by path. There is no URL install yet: `Frameworks/` is a build
output and is gitignored, so a git tag carries no XCFramework for
`binaryTarget` to resolve — see [Publishing](#publishing) for what hosting one
would take.

```swift
dependencies: [
    .package(path: "../fedimint-sdk/ios")
],
targets: [
    .target(name: "MyApp", dependencies: [
        .product(name: "FedimintSdk", package: "FedimintSdk")
    ])
]
```

In Xcode: **File → Add Package Dependencies… → Add Local…** and pick this
directory.

Requires iOS 15+, or macOS 13+ **on Apple Silicon** — the XCFramework ships
no Intel macOS slice, for the reason given beside `platforms:` in
[`Package.swift`](Package.swift). Nothing else is needed at the call site: the
system frameworks the native library pulls in (`SystemConfiguration`, `Security`
and `Network`) are declared in the XCFramework's own modulemap, and the C++
runtime rocksdb and aws-lc need is linked by the package itself, so there are no
"Link Binary With Libraries" entries to add.

## Scope

The whole surface of the Rust crate, unfiltered. The entry point opens storage
and establishes the seed; `Federation` carries the rest.

| Swift                                                                  | What it does                                 |
| ---------------------------------------------------------------------- | -------------------------------------------- |
| `createFedimintSdk(dataDir:mnemonic:)`                                 | Opens storage; establishes the seed          |
| `sdk.exportMnemonic()`                                                 | The instance's `Mnemonic` handle, to back up |
| `sdk.preview(invite:)`                                                 | Reads a federation's config without joining  |
| `sdk.join(invite:)`                                                    | Joins it, returns a `Federation` handle      |
| `sdk.recover(invite:)` / `resumeRecovery(id:)` / `recoveryStatus(id:)` | Restore a wallet from its seed               |
| `Mnemonic.generate()` / `.fromWords(_:)` / `mnemonic.words()`          | make / restore / read a seed                 |
| `InviteCode.parse(code:)` / `.federationId()` / `.display()`           | parse / inspect / render a code              |
| `Notes.parse(notes:)` / `.value()` / `.display()`                      | parse / inspect / render ecash               |

`Mnemonic`, `InviteCode` and `Notes` are **opaque handles**, not strings — an
invite code is a bearer credential, ecash notes are a bearer token, and a seed is
a secret the Rust type keeps behind a zeroizing buffer, so none of them is handed
to Swift as a loggable value. Build one with its constructor, then pass the
handle. Each throws on invalid input, before any real work runs.

The seed is established by `createFedimintSdk`: pass a `Mnemonic` to restore, or
`nil` to load the seed the directory already holds — or, over an empty directory,
generate and persist a fresh one. `sdk.exportMnemonic()` reads it back.

The `Federation` that `join` returns carries the rest: `balance()`,
`balanceUpdates()`, `capabilities()`, `meta()`, `activity(cursor:limit:)`,
`operation(id:)`, and the `ecash()`, `lightning()` and `onchain()` facades (each
`nil` when the federation lacks that module), whose `quote` → `send` and
`receive` calls return operation handles to observe with `state()`, `updates()`
and `awaitFinal()`. The demo app (`ios/Demo`) drives each of them once.

## Using it

```swift
import FedimintSdk

// 1. Open the SDK over an app-private directory. `nil` loads the seed already
//    there, or generates one when the directory is empty; pass a Mnemonic
//    (Mnemonic.fromWords(words:)) to restore.
let dataDir = FileManager.default
    .urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    .appendingPathComponent("fedimint")
try FileManager.default.createDirectory(at: dataDir, withIntermediateDirectories: true)

let sdk = try await createFedimintSdk(dataDir: dataDir.path, mnemonic: nil)

// 2. Show the seed so the user can write it down — `words()` is the deliberate
//    step that takes it out of the SDK's care as plain strings.
let words: [String] = sdk.exportMnemonic().words()

// 3. Show the user what they are about to join.
let invite = try InviteCode.parse(code: inviteString)   // throws on a malformed code
let preview = try await sdk.preview(invite: invite)     // a struct, not an object
print("\(preview.name ?? "unnamed") on \(preview.network)")
print("\(preview.guardians) guardians, modules \(preview.modules)")

// 4. Join, then use the facades it offers.
let federation = try await sdk.join(invite: invite)
let balanceMsats: UInt64 = try await federation.balance()
```

Every call that touches disk or the network is `async throws`, so it needs an
`await` and a `Task` — there is no synchronous variant and no completion-handler
variant.

### Error handling

Every fallible call throws `SdkError`.

The Rust type is `crate::Error`, and UniFFI's Swift backend does not rename it —
so the generated class is literally `Error`, which is ambiguous with
`Swift.Error` in any file that writes `catch let e as Error`. `SdkError` is a
type alias for it, declared in
[`Version.swift`](Sources/FedimintSdk/Version.swift); use that name. It is the
direct counterpart of the `import org.fedimint.sdk.Exception as SdkException`
that [`android/README.md`](../android/README.md) tells Kotlin callers to write,
for exactly the same reason. (Kotlin's backend renames every Rust `*Error` to
`*Exception` on its own; Swift's renames nothing.)

It carries the Rust `ErrorCode` as a real Swift enum from `code()` and the
human-readable message from `reason()`:

```swift
do {
    _ = try await sdk.join(invite: InviteCode.parse(code: inviteString))
} catch let error as SdkError {
    switch error.code() {
    case .invalidInput:          show("That invite code isn't valid.")
    case .alreadyJoined:         show("You're already in this federation.")
    case .federationUnreachable,
         .timeout:               show("Couldn't reach the federation.")
    case .unsupportedFederation: show("This app can't work with that federation.")
    default:                     show(error.reason())
    }
}
```

An exception always means _the call_ failed. It never means value moved badly —
that distinction is the core convention of the Rust crate.

`ErrorCode` is `#[non_exhaustive]` in Rust, so a binding pinned to an older SDK
cannot decode a code added since; the generated Swift enum is closed regardless,
which is why the `default:` arm above is not optional. Regenerate the bindings
alongside the crate.

### Three things that will bite

**Quotes are single use.** In plain Rust `send` takes its quote by value, so the
first attempt consumes it whatever the outcome. Swift only ever holds a shared
handle, so the quote carries the guard instead: the second `send` on the same
quote throws `.quoteExpired`. A failed attempt may already have moved funds, so
the only safe retry is a fresh quote.

**`Amount` and `Sats` are both `UInt64`.** They are distinct Rust types that
cross the boundary as their primitive shape, so the compiler cannot tell them
apart for you. `Amount` is **millisatoshis**; `Sats` is satoshis. Only
`Onchain.quote(address:amount:)` takes `Sats` — everything else takes `Amount`.

**`shutdown()` is not enough to reopen a directory.** It releases the storage
lock, but the underlying store stays open until every `Sdk` and `Federation`
handle over it has actually been _dropped_, and a `createFedimintSdk` against
the same location before then **waits** rather than throwing. In Swift a handle
is dropped by ARC, so `shutdown()` alone leaves it alive for the rest of the
enclosing scope:

```swift
var sdk: Sdk? = try await createFedimintSdk(dataDir: path, mnemonic: nil)
// ... use it ...
try await sdk?.shutdown()
sdk = nil                      // <- this is what actually closes the store
```

Skipping the release only matters if the same process reopens the same
directory; across process restarts, which is the normal case, there is nothing
to do. `.storageInUse` is the _other_ case — a second opener while the first
still holds the lock — and that one throws immediately rather than waiting.

## Building

The native library and the Swift are **generated**, not committed. Building them
is two steps, and the split is deliberate — the same split
[`android/README.md`](../android/README.md) describes:

1. **The native libraries.** `cargo` cross-compiles `libfedimint_sdk.a` for each
   Apple target. This is the expensive half — rocksdb and aws-lc are built from
   C — and it is not specific to Swift.
2. **The bindings.** `uniffi-bindgen` reads the UniFFI metadata **out of the
   `.a` built in step 1**, not out of the crate source, so the Swift cannot
   drift from the binary it will load on the device. This half takes seconds.

Unlike Android, step 1 does **not** go through Nix. Every Apple target compiles
against an SDK that ships inside Xcode and cannot live in the nix store, so there
is no cacheable derivation to build — [`nix/ffi.nix`](../nix/ffi.nix) stays
Android-only. `nix develop .#ios` supplies the Apple Rust targets and the
cmake/perl/go that aws-lc and rocksdb want; Xcode supplies the SDKs.

```sh
just build-ios-lib        # cargo -> libfedimint_sdk.a per target, + lipo'd sim slice
just build-swift-bindings # uniffi-bindgen over that .a  -> FedimintSdk.swift
just build-swift          # both, then xcodebuild -create-xcframework
just test-swift           # build-swift, then `swift test` on the macOS slice
just build-ios-demo       # build-swift, then xcodegen + compile the demo
```

A full build produces four targets. To iterate faster, narrow it:

```sh
IOS_TARGETS="aarch64-apple-darwin" just build-ios-lib   # host slice only
```

That one slice is enough for `swift test`, which runs on macOS and needs no
simulator. CI ([`swift-sdk.yaml`](../.github/workflows/swift-sdk.yaml)) uses the
same knob: two targets on a pull request, all four on `main`.

A narrowed build produces a narrowed XCFramework — it does **not** keep slices
from an earlier run. That is deliberate: `build-ios-lib.sh` records the triples
it built in `target/apple-slices.txt`, and both the bindings step and the
XCFramework assembly key off that record rather than off which archives happen
to be on disk. Otherwise a populated `target/` would let a subset build ship
stale machine code, and — worse — let `uniffi-bindgen` read its metadata out of
an archive that is not the one being shipped, which is the exact drift the
two-step split exists to prevent. Run a full build when you want a full
framework back.

Non-Nix escape hatch: the scripts are plain `cargo` and never call `nix`, so
running them directly works on any machine whose toolchain already has the Apple
targets — the counterpart of `build-android-sdk.sh --local`:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios
./scripts/build-ios-sdk.sh
```

`.#ios` exists for the machine that does not have those, and to pin the
`cmake`/`perl`/`go` that aws-lc-sys and rocksdb need; it is not a requirement of
the build. CI takes the direct route for the same reason — there is no cacheable
Nix output here to be worth installing Nix for.

Requires macOS with Xcode either way. `xcodebuild -checkFirstLaunchStatus` must
exit 0 — run `xcodebuild -runFirstLaunch` once if it does not.

## Layout

```
ios/
├── Package.swift                     SPM manifest
├── README.md
├── Sources/FedimintSdk/
│   ├── FedimintSdk.swift             generated, gitignored
│   └── Version.swift                 the one committed Swift file; says why
├── Frameworks/
│   ├── Headers/                      generated, gitignored
│   │   ├── FedimintSdkFFI.h
│   │   └── module.modulemap
│   └── FedimintSdkFFI.xcframework/   generated, gitignored
│       ├── ios-arm64/                physical iPhone / iPad
│       ├── ios-arm64_x86_64-simulator/
│       └── macos-arm64/              what `swift test` runs against
├── Tests/FedimintSdkTests/           offline tests: no federation, no network
└── Demo/
    ├── project.yml                   XcodeGen spec (committed)
    ├── FedimintDemoApp.swift
    ├── ContentView.swift             every facade, one screen
    └── FedimintDemo.xcodeproj/       generated, gitignored
```

## What's not here yet

No ergonomic Swift layer — no `AsyncSequence` over the `*Updates` cursors, no
typed wrappers around `Amount`/`Sats`, no protocol unifying the seven
`*Operation` classes. That is deliberate: the generated bindings are the API,
Kotlin has no such layer either, and adding one on only one platform would put
the two SDKs out of step. [`DECISION.md`](../rust/uniffi-bindgen/DECISION.md) is
where to revisit it.

## Publishing

Not wired up yet. [`swift-sdk.yaml`](../.github/workflows/swift-sdk.yaml) builds
the XCFramework, runs the tests and compiles the demo — it does not publish.
Shipping this as a versioned SPM package means hosting the XCFramework as a
release asset and switching `Package.swift`'s `binaryTarget` from `path:` to
`url:`/`checksum:`; `Version.swift`'s `fedimintSdkVersion` names the version for
whenever that happens.
