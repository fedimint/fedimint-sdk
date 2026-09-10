# Fedimint Android SDK

A Kotlin/Android SDK generated with [UniFFI](https://mozilla.github.io/uniffi-rs/)
from [`fedimint-sdk`](../rust/fedimint-sdk)'s `uniffi` feature.

There is **no hand-written Kotlin here**. The `#[uniffi::export]` blocks in
[`fedimint-sdk`](../rust/fedimint-sdk) export that crate's real methods as-is and
hand out its own types — `Sdk`, `Federation`, `Mnemonic`, `InviteCode`,
`FederationPreview`, `FederationId`, `Network`, `Error`, `ErrorCode` — so this SDK
is a view of that API rather than a copy of it that could drift.

## Scope

Three `Sdk` methods, plus the constructor and the two value types they need:

| Kotlin                                                              | What it does                                  |
| ------------------------------------------------------------------- | --------------------------------------------- |
| `createFedimintSdk(dataDir, seed?)`                                 | Opens storage; `build()` establishes the seed |
| `sdk.exportMnemonic()`                                              | The instance's `Mnemonic` handle, to back up  |
| `sdk.preview(invite)`                                               | Reads a federation's config without joining   |
| `sdk.join(invite)`                                                  | Joins it, returns a `Federation` handle       |
| `Mnemonic.generate()` / `.fromWords(List)` / `mnemonic.words()`     | make / restore / read a seed                  |
| `InviteCode.parse(String)` / `invite.federationId()` / `.display()` | parse / inspect / render a code               |

`Mnemonic` and `InviteCode` are **opaque handles**, not strings — an invite code
is a bearer credential and a seed is a secret that the Rust type keeps behind a
zeroizing buffer, so neither is handed to Kotlin as a loggable value. Build one
with its constructor (`InviteCode.parse(...)` / `Mnemonic.generate()` /
`Mnemonic.fromWords(...)`), then pass the handle. Both throw on an invalid input,
before any real work runs.

The seed is established by `createFedimintSdk`: pass a `Mnemonic` to restore, or
`null` to load the seed the directory already holds — or, over an empty
directory, generate and persist a fresh one. `sdk.exportMnemonic()` reads it back.

Balances, ecash, Lightning, on-chain and history are **not** here. The
`Federation` that `join` returns is an **opaque handle with no methods yet** —
those facades are still `unimplemented!()` in the Rust crate and land in a later
stage. Hold it (and `close()` it when done); there is nothing to call on it now.

## Using it

```kotlin
import org.fedimint.sdk.*
import org.fedimint.sdk.Exception as SdkException   // shadows kotlin.Exception otherwise

// 1. Open the SDK over an app-private directory. `null` loads the seed already
//    there, or generates one when the directory is empty; pass a Mnemonic
//    (Mnemonic.fromWords(userWords)) to restore.
val sdk: Sdk = createFedimintSdk(context.filesDir.path, null)

// 2. Show the seed so the user can write it down — `words()` is the deliberate
//    step that takes it out of the SDK's care as plain strings.
val words: List<String> = sdk.exportMnemonic().words()

// 3. Show the user what they are about to join. FederationId is a String alias.
val invite = InviteCode.parse(inviteString)      // throws on a malformed code
val preview: FederationPreview = sdk.preview(invite)
println("${preview.name ?: "unnamed"} on ${preview.network}")   // Network enum
println("${preview.guardians} guardians, modules ${preview.modules}")
preview.meta["welcome_message"]?.let(::println)

// 4. Join. `federation` is opaque for now — hold it for a later stage.
val federation: Federation = sdk.join(invite)

// 5. Release the handles and the lock on dataDir.
federation.close()
sdk.close()
```

`createFedimintSdk`, `preview` and `join` are `suspend` functions doing real disk
and network work — call them from a coroutine, as the demo does. `exportMnemonic()`
and the `Mnemonic` / `InviteCode` methods are plain accessors.

### Error handling

Every fallible call throws `org.fedimint.sdk.Exception` — UniFFI's Kotlin backend
renames every Rust `*Error` type to `*Exception`, so `crate::Error` lands as
`Exception`; import it aliased so it does not shadow `kotlin.Exception`. It
carries the Rust `ErrorCode` as a real Kotlin enum from `code()` and the
human-readable message from `reason()`:

```kotlin
try {
    sdk.join(InviteCode.parse(inviteString))
} catch (e: SdkException) {
    when (e.code()) {
        ErrorCode.INVALID_INPUT          -> showError("That invite code isn't valid.")
        ErrorCode.ALREADY_JOINED         -> showError("You're already in this federation.")
        ErrorCode.FEDERATION_UNREACHABLE,
        ErrorCode.TIMEOUT                -> showError("Couldn't reach the federation.")
        ErrorCode.UNSUPPORTED_FEDERATION -> showError("This app can't work with that federation.")
        else                             -> showError(e.reason())
    }
}
```

An exception always means _the call_ failed. It never means value moved badly —
that distinction is the core convention of the Rust crate.

`FederationId` is a `String` typealias; `Mnemonic`, `InviteCode`, `Sdk` and
`Federation` are opaque handle classes; `FederationPreview` is a data class and
`Network` / `ErrorCode` are plain enums — all regenerated from the crate.

`ErrorCode` is `#[non_exhaustive]` in Rust, so a binding pinned to an older SDK
cannot decode a code added since. Regenerate the bindings alongside the crate.

## Building

The native libraries and the Kotlin are **generated**, not committed. Building
them is two steps, and the split is deliberate:

1. **The native library.** The cross-compile lives in
   [`nix/ffi.nix`](../nix/ffi.nix) as cacheable derivations, so nothing compiles
   locally when the Cachix cache is warm
   ([`android-native.yaml`](../.github/workflows/android-native.yaml) keeps it
   warm on `main`). This is the expensive half — rocksdb and aws-lc are built
   from C — and it is not specific to Kotlin.
2. **The bindings.** `uniffi-bindgen` reads the UniFFI metadata **out of the
   `.so` built in step 1**, not out of the crate source, so the Kotlin cannot
   drift from the binary it will load on the device. This half takes seconds.

```sh
just build-android-so        # .#fedimint-sdk-android-jni     ->  jniLibs/ only
just build-kotlin-bindings   # uniffi-bindgen over that .so   ->  java/ only
just build-kotlin            # both, in that order
just build-android-aar       # build-kotlin, then ./gradlew :fedimint-sdk:assembleRelease
just test-kotlin             # build-kotlin, then compile the library + demo
```

CI runs those same two scripts as two workflows —
[`android-native.yaml`](../.github/workflows/android-native.yaml) builds the
`.so` and uploads it,
[`kotlin-sdk.yaml`](../.github/workflows/kotlin-sdk.yaml) calls that workflow
and generates the Kotlin from the artifact — so the shared, costly half is
built once and any binding generator added later starts from the same binary.

Non-Nix escape hatch: `just build-android-local` (`scripts/generate-android-so.sh`
via `cargo-ndk` in the `.#android` shell).

Gradle needs a host JDK 17.

## Layout

```
android/
├── settings.gradle.kts
├── gradle/libs.versions.toml        version catalog
└── fedimint-sdk/                    the library module → AAR
    ├── build.gradle.kts
    ├── consumer-rules.pro           R8 keep rules shipped to consumers
    └── src/main/
        ├── AndroidManifest.xml
        ├── jniLibs/<abi>/*.so       generated, gitignored
        └── java/org/fedimint/sdk/   generated, gitignored
```

## What's not here yet

`Federation` is an opaque handle — its facades (`balance`, `ecash`, `lightning`,
`onchain`, `meta`, `activity`) are `unimplemented!()` in the Rust crate and land
in a later stage. The value types those methods use (`Amount`, `Bolt11Invoice`,
`Address`, `Notes`, `Txid`, `OperationId`, `Cursor`, `Timestamp`) get their FFI
mapping when the facade that returns them is exported. iOS bindings are a
separate follow-up off the same `uniffi` feature.

## Publishing

Not wired up yet. [`kotlin-sdk.yaml`](../.github/workflows/kotlin-sdk.yaml)
builds the AAR and compiles the demo against the generated bindings — it does
not publish. `libs.versions.toml`'s `fedimintSdk` names the version for
whenever it is.
