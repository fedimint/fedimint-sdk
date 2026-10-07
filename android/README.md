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

The `Federation` that `join` returns carries the rest of the surface: `balance()`,
`capabilities()`, `activity()`, `meta()`, `operation(id)`, and the `ecash()`,
`lightning()` and `onchain()` facades (each `null` when the federation lacks that
module), whose `quote` → `send` and `receive` calls return operation handles to
observe with `state()`, `updates()` and `awaitFinal()`. The example app
(`android/app`) drives each of them once.

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

// 4. Join, then use the facades it offers.
val federation: Federation = sdk.join(invite)
val balanceMsats: ULong = federation.balance()

// 5. Release the handles and the lock on dataDir.
federation.close()
sdk.close()
```

`createFedimintSdk`, `preview` and `join` are `suspend` functions doing real disk
and network work — call them from a coroutine, as the example app does. `exportMnemonic()`
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
`Federation` are opaque handle classes, and so is `Notes`: ecash notes are a
bearer token, so they come out only through `display()` and never through a
record's `toString()`; `FederationPreview` is a data class and
`Network` / `ErrorCode` are plain enums — all regenerated from the crate.

`ErrorCode` is `#[non_exhaustive]` in Rust, so a binding pinned to an older SDK
cannot decode a code added since. Regenerate the bindings alongside the crate.

### Permissions and setup

The library's manifest declares two permissions, and Gradle merges them into
your app's manifest; both are normal permissions, so there is no runtime prompt:

| Permission             | Why                                                           |
| ---------------------- | ------------------------------------------------------------- |
| `INTERNET`             | Talking to guardians and gateways                             |
| `ACCESS_NETWORK_STATE` | Reading the active network's DNS servers for iroh connections |

Don't strip `ACCESS_NETWORK_STATE` (for example with `tools:node="remove"`).
Android has no readable `resolv.conf`, so the SDK asks `ConnectivityManager` for
the DNS servers. Without the permission that call fails, and iroh falls back to
public DNS servers with only a logcat warning.

There is no initialization call. The SDK finds your `Application` itself when
`createFedimintSdk` runs, so it is fine to touch other bindings, such as
`InviteCode.parse`, earlier in startup.

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
just build-android-so         # .#fedimint-sdk-android-jni     ->  jniLibs/ only
just build-android-bindings   # uniffi-bindgen over that .so   ->  java/ only
just build-android-sdk        # both, in that order
just build-android-aar        # build-android-sdk, then ./gradlew :fedimint-sdk:assembleRelease
just test-android-sdk         # build-android-sdk, then compile the library + example app
just build-android-apk        # the example APK, in the lean `.#android` shell
just test-android-e2e         # build-android-apk, then drive the example app on an emulator
```

`test-android-e2e` is the only one of these that needs a device: it installs the
example app on an emulator and drives it with the Appium suite in
[`js/android/integration-tests`](../js/android/integration-tests), so the
bindings are loaded and the calls execute rather than merely compiling. It runs
in the `.#android-tests` shell, which adds the emulator, a system image and
devimint to what `.#android` provides.

The APK is built by `build-android-apk` in the lean shell and only _installed_ by
the device run, never built there: a Gradle build alongside an emulator and a
devimint federation starved the emulator until Android's System UI stopped
responding. Both shells carry a JDK for Gradle, so `.#android` is enough to
assemble the app without a host one.

CI runs those same two scripts as separate jobs —
[`android-native.yaml`](../.github/workflows/android-native.yaml) builds the
`.so` and uploads it,
[`android-sdk.yaml`](../.github/workflows/android-sdk.yaml) calls that workflow
and generates the Kotlin from the artifact — so the shared, costly half is
built once and any binding generator added later starts from the same binary.
The Kotlin is generated once too: the AAR job and
[`android-apk.yaml`](../.github/workflows/android-apk.yaml) both download it
rather than regenerate it, and `android-apk.yaml` is the one place the example
app is compiled. [`android-e2e.yml`](../.github/workflows/android-e2e.yml) then
installs that APK on an emulator.

Non-Nix escape hatch: `just build-android-local` (`scripts/build-android-sdk.sh
--local`, via `cargo-ndk` in the `.#android` shell).

Gradle needs a JDK 17. `just build-android-apk` gets one from the `.#android` shell; the recipes that run
Gradle directly (`test-android-sdk`, `build-android-aar`) use the host's, so those still need one installed.

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

iOS bindings are a separate follow-up off the same `uniffi` feature.

## Publishing

The AAR is published to Maven Central as `org.fedimint:sdk`, through
the Sonatype Central Portal:

```kotlin
dependencies {
    implementation("org.fedimint:sdk:0.1.0-beta.1")
}
```

The version above is the first one this repository will release. It resolves
only once that release has actually been published on Maven Central; until
then, and for newer versions, check
[Maven Central](https://central.sonatype.com/artifact/org.fedimint/sdk) for
what exists.

### Versioning

The Android SDK has its own version, `fedimintSdk` in
[`gradle/libs.versions.toml`](gradle/libs.versions.toml), independent of
[`rust/fedimint-sdk/Cargo.toml`](../rust/fedimint-sdk/Cargo.toml)'s. It follows
[semantic versioning](https://semver.org), `MAJOR.MINOR.PATCH`, and describes
the **Kotlin API**. The Kotlin API is generated from the Rust exports, so a
change on the Rust side counts if it changes what Kotlin callers see.

| Change since the last release                                                                | Before 1.0.0 | From 1.0.0 |
| -------------------------------------------------------------------------------------------- | ------------ | ---------- |
| Breaks callers: removes or renames a class, function, parameter or enum case, changes a type | `minor`      | `major`    |
| Adds to the API without breaking anything                                                    | `minor`      | `minor`    |
| Fixes behaviour, with no change to the API                                                   | `patch`      | `patch`    |

Going to 1.0.0 is a `major` bump, made on purpose once the API is stable.
Before that, a `minor` bump is how a breaking change is signalled, as semver
allows for 0.x.

There are two kinds of release: betas, `X.Y.Z-beta.N`, for testing, then
`X.Y.Z` itself. Nothing else (no alpha, no rc) is accepted. Both kinds are
permanent on Central, and every release must be newer than all the ones
before it. A version moves like this:

```
0.1.0 --minor, as beta--> 0.2.0-beta.1 --beta--> 0.2.0-beta.2 --release--> 0.2.0
0.2.0 --patch-------------------------------------------------------------> 0.2.1
```

A beta is always either continued (`beta`) or released (`release`). There is
no `patch`, `minor` or `major` in the middle of one.

Don't edit the version by hand. Run
[`android-sdk-version-bump.yaml`](../.github/workflows/android-sdk-version-bump.yaml)
(Actions → Android SDK Version Bump), pick the bump and whether a new version
starts as a beta, and it opens a pull request with the change and drafted
release notes. It refuses a bump that doesn't fit, and a version that is
already taken.
[`scripts/android-sdk-version.sh`](../scripts/android-sdk-version.sh) holds
these rules for both that workflow and the release, and works locally too:

```sh
scripts/android-sdk-version.sh current          # 0.1.0-beta.1
scripts/android-sdk-version.sh next release     # 0.1.0
scripts/android-sdk-version.sh next minor --beta
```

Snapshots are separate from all of this; see below.

### Releases

[`android-sdk-release.yaml`](../.github/workflows/android-sdk-release.yaml) does
the release. It first runs the whole
[`android-sdk.yaml`](../.github/workflows/android-sdk.yaml) chain on the release
commit, including the emulator run. Only if that passes does it build the AAR
again from the native libraries and bindings that run tested, then sign and
upload it. The upload is not released
automatically. It waits on central.sonatype.com until someone publishes it
there, because a version released to Central can never be changed or deleted.
The workflow also does not wait for Central to validate it, so a green run
means uploaded, not validated.

Every release has notes in [`CHANGELOG.md`](CHANGELOG.md), one `## <version>`
section each. They also become the version's GitHub Release, and the tag's
message. A release is refused without them.

To release:

1. Run the Android SDK Version Bump workflow (see Versioning). Its pull request
   sets the version and drafts the version's section in `CHANGELOG.md` from
   the commits since the last release that touched the SDK.
2. In that pull request, rewrite the drafted notes for users and delete the
   draft comment. Then merge it. Merging is the decision to release:
   [`android-sdk-tag.yaml`](../.github/workflows/android-sdk-tag.yaml) tags the
   merge commit `android-sdk-v<version>` and starts the release on it.
3. Approve the `maven-central` environment when the release run asks.
4. On central.sonatype.com → Publishing → Deployments, wait for the
   deployment to show `VALIDATED` (a `FAILED` one lists the reason), then
   press Publish.
5. Publish the version's draft GitHub Release, which the run created.

The release is refused if the tag and the catalog disagree, if the version is
not a beta or a release, if it is not newer than every `android-sdk-v*` tag
already pushed, or if its notes are missing or still a draft.

The Android SDK Tag workflow tags automatically only right after a version
bump that follows a release. For anything else, run it by hand on `main`
(Actions → Android SDK Tag → Run workflow). The same checks apply. That
covers two cases:

- **The first release.** `0.1.0-beta.1` and its notes are already in place, so
  there is no bump pull request to merge.
- **A release whose automatic tagging failed,** for example because its notes
  were merged as a draft. Fix the notes in a pull request, then run it.

Pushing a `android-sdk-v*` tag by hand also still releases.

The two workflows behave differently when run by hand:

- **Android SDK Tag, run by hand, releases for real.** It pushes the tag and
  starts Android SDK Release with publishing on.
- **Android SDK Release, run by hand with "publish" unticked, is a dry run.**
  It signs the artifacts into a local Maven repository and uploads that
  repository as a workflow artifact to inspect. Nothing is tagged or
  uploaded to Central.

The POM, signing and the Central setup live in
[`fedimint-sdk/build.gradle.kts`](fedimint-sdk/build.gradle.kts), which also
lists the credentials Gradle expects.

To do the same check locally, publish to a scratch repository from the
`android/` directory, where the Gradle wrapper is. The native libraries and
bindings must be built first (`just build-android-sdk`). Any throwaway
signing key will do, because a non-SNAPSHOT version is always signed:

```sh
cd android
ORG_GRADLE_PROJECT_signingInMemoryKey="$(cat /path/to/throwaway-key.asc)" \
  ./gradlew :fedimint-sdk:publishToMavenLocal -Dmaven.repo.local="$PWD/build/m2"
```

### Snapshots

[`android-sdk-snapshot.yaml`](../.github/workflows/android-sdk-snapshot.yaml)
publishes a snapshot of a branch's current commit to Central's snapshots
repository. It only runs by hand: Actions → Android SDK Snapshot → Run
workflow, on the branch you want, `main` included. Nothing publishes a
snapshot automatically. Like a release, it publishes only after the whole
`android-sdk.yaml` chain has passed on that commit, and only after a reviewer
approves the `maven-central` environment. The upload uses the same Central
token as releases, and the branch's own build code runs with it.

The version is `<branch>-<commit>-SNAPSHOT`:

- `<branch>` is the branch name, with every character a Maven version cannot
  hold (such as `/` or `#`) replaced by `-`.
- `<commit>` is the first 12 characters of the commit's hash.

So `main` at commit `85bd33d6df4ba32…` publishes `main-85bd33d6df4b-SNAPSHOT`,
and `feat/x` at the same commit publishes `feat-x-85bd33d6df4b-SNAPSHOT`.
Every commit gets its own version, so there is no version that follows a
branch: to try a newer commit, switch to its version.

To find the exact version, open the Android SDK Snapshot run for that commit
(Actions → Android SDK Snapshot). Its summary shows the published version with
a ready-to-copy dependency line. To work it out yourself instead:

```sh
printf '%s-%s-SNAPSHOT\n' "$(git branch --show-current | tr -c 'A-Za-z0-9._\n-' '-')" "$(git rev-parse HEAD | cut -c1-12)"
```

Snapshots are deleted after about 90 days, so they are for trying unreleased
work, not for shipping:

```kotlin
repositories {
    maven("https://central.sonatype.com/repository/maven-snapshots/")
}
dependencies {
    implementation("org.fedimint:sdk:main-85bd33d6df4b-SNAPSHOT")
}
```

Locally, `-Psnapshot=<name>` builds `<name>-SNAPSHOT`, and it needs no
signing key.
