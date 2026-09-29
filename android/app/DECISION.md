# Android reference wallet: design decisions

Tracking issue: [#399](https://github.com/fedimint/fedimint-sdk/issues/399).

`android/app` is turning from a one-screen SDK test harness into a reference wallet:
an app someone building on the Fedimint Android SDK can read and copy. This file
records the decisions behind its structure and why each alternative was turned down,
so reviewers can check the reasoning and later contributors can extend it without
re-arguing it.

## Constraints the design has to respect

- **One SDK instance per process.** `createFedimintSdk` takes an exclusive lock on
  its data directory (`STORAGE_IN_USE` otherwise). Whatever owns the SDK has to
  outlive screens, rotation and activity recreation.
- **The SDK surface is async and streaming.** Calls are `suspend fun`s, and live
  data (`balanceUpdates`, `federationStatusUpdates`, an operation's `updates()`)
  comes from subscriptions you call `next()` on repeatedly.
- **CI compiles this app to prove the bindings work.**
  `.github/workflows/kotlin-sdk.yaml` runs `:app:assembleDebug`, so the app has to
  keep calling the whole binding surface at every commit.
- **Recovery phrases are secrets.** Anyone who sees the words owns the funds.
- **The Kotlin toolchain is pinned** (Kotlin 2.0.21, AGP 8.13, compileSdk 36,
  build-tools 36.0.0 from the nix shell).

## Decisions

### 1. Jetpack Compose for the UI (not XML Views)

The harness uses XML layouts and `findViewById`, and updates each widget by hand.
A wallet with 10+ screens and several live streams needs UI that redraws itself
from state.

- **Why:** a Compose screen is a function of its state. A `StateFlow` from the
  ViewModel is collected with `collectAsStateWithLifecycle()`, and the screen
  redraws when the balance or an operation's state changes, with no manual view
  updates. Compose is also what Google recommends for new apps and what readers of
  a reference app will expect.
- **Rejected: Views + Fragments.** It works, but it needs an XML layout, a Fragment
  and view-binding code per screen: roughly twice the code for the same
  behaviour, and it looks dated in a reference app.

### 2. One activity, one navigation graph, type-safe routes

`MainActivity` hosts a single Navigation Compose `NavHost`. Destinations are
`@Serializable` objects in `ui/nav/Routes.kt`.

- **Why:** a single activity gives one back stack, one place to decide the start
  screen, and screens that share the app-wide SDK. Type-safe routes (Navigation
  2.8+) turn a screen's arguments into a data class checked at compile time. When
  "Operation detail" needs an operation id, a missing or mistyped argument is a
  compile error, not a crash.
- **Rejected: string routes** (`"operation/{id}"`), which are typo-prone and only
  fail at runtime. **Rejected: an activity per screen**, which makes sharing the
  SDK and the back-stack rules harder.

### 3. One app-scoped `WalletSession` owns the SDK

`wallet/WalletSession.kt` is the only code that creates or holds the `Sdk`. It is
created once in `FedimintApp` (the `Application`) and exposes main-safe methods.

- **Why:** this follows directly from the one-instance lock. An owner scoped to the
  activity or a screen would re-open the SDK on rotation, or leave two instances
  fighting over the lock. Moving work onto `Dispatchers.IO` inside the session means
  no screen needs to know which calls block.
- **How it grows:** federation handles, balance streams and so on become methods and
  flows on the session, or on small classes beside it (for example a
  `FederationRepository`) once one file gets too big.

### 4. A ViewModel per screen, exposing one `StateFlow<UiState>`

Each screen file holds its ViewModel and its composable. The ViewModel exposes an
immutable `UiState` and plain functions for user actions (`create()`, `verify()`,
`restore()`).

- **Why:** this is the standard Android unidirectional data flow. State survives
  rotation, work runs in `viewModelScope` and is cancelled when the user leaves the
  screen, and composables stay free of logic. It also keeps each feature
  self-contained, so adding a screen means adding a file.
- One-off outcomes (for example "wallet created, move on") are fields in `UiState`,
  acted on with `LaunchedEffect`. That is simpler than a separate event channel and
  can't lose an event across rotation.

### 5. Manual dependency injection (no Hilt or Koin)

`AppContainer` in `FedimintApp.kt` builds the object graph, and `appViewModel { }`
(`ui/common/Compose.kt`) hands it to ViewModels.

- **Why:** the graph is one object today and at most a handful later. Hilt would add
  kapt/KSP, annotations and generated code, and a reader would have to learn that
  before seeing a single SDK call. For a reference app, what matters is the SDK.
- **When to revisit:** if the graph grows beyond what one small container can wire
  by hand. `appViewModel { }` mirrors `hiltViewModel()`, so a later move would only
  touch the call sites.

### 6. Build in place in `android/app`, and keep the harness until the end

The old screen moved unchanged to `harness/HarnessActivity.kt`, and the wallet's
Home screen links to it ("Developer tools").

- **Why:** CI's `:app:assembleDebug` only proves the bindings are usable because the
  app calls every API. While the wallet screens are being built, the harness keeps
  the untouched APIs compiled and reachable on a device. It is deleted in the last
  step, once the wallet covers the whole surface.
- The harness opens its SDK over `files/harness`, and the wallet uses `files/wallet`.
  Separate directories mean the two never contend for the storage lock, and the
  harness can't touch the wallet's seed.
- **Rejected: a separate `android/wallet` module.** That means two apps to keep
  building and a CI change, for no benefit once the harness is gone.

### 7. Library versions stay on Kotlin 2.0

Compose BOM 2024.12.01, Navigation 2.8.5, Lifecycle 2.8.4, Activity 1.9.3,
kotlinx-serialization 1.7.3. The Compose compiler and serialization plugins use the
pinned Kotlin version.

- **Why:** these are the newest releases built with Kotlin 2.0. Newer ones are
  compiled by Kotlin 2.1+, whose metadata the pinned 2.0.21 compiler can't read.
  Staying here means the SDK module, the nix shell and CI need no toolchain bump.
  Upgrading Kotlin is a separate, repo-wide change.

### 8. Handling recovery phrases

- The Backup, Verify and Restore screens set `FLAG_SECURE` (`SecureScreen()`): no
  screenshots, no screen recording, and a blank recent-apps thumbnail.
- The phrase is held only in ViewModel memory, never in `rememberSaveable` or saved
  instance state, because Android can write saved state to disk.
- Seed text fields use a password keyboard type with autocorrect off, so most
  keyboards neither suggest nor learn the words. The field isn't masked, because the
  user has to check what they typed.
- The backup is confirmed by asking for 3 words at random positions. That proves a
  complete, ordered copy without making the user retype all 12 or 24 words.
- "Backed up" is app state in `SharedPreferences`. The SDK has no notion of it.

### 9. Which screen the app starts on

`LaunchViewModel` (`ui/nav/WalletApp.kt`) decides once per launch:

| State on disk | Start screen |
|---|---|
| No wallet directory | Welcome (create or restore) |
| Wallet exists, backup not confirmed | Backup (the app was closed mid-onboarding) |
| Wallet exists, backup confirmed | Home |

The SDK has no "does a seed exist" call short of opening it, so "a wallet exists"
means the SDK's data directory is non-empty. After creating a wallet, Welcome is
removed from the back stack, because the seed now exists and "create or restore"
no longer applies. After a verified backup or a restore, the whole onboarding stack
is cleared, so Back from Home leaves the app.

## Package layout

```
org.fedimint.demo
├── FedimintApp.kt          Application + AppContainer (manual DI)
├── MainActivity.kt         the single Compose activity
├── wallet/                 SDK ownership: WalletSession, and repositories as they come
├── ui/
│   ├── nav/                routes, NavHost, start-screen decision
│   ├── theme/              Material 3 theme
│   ├── common/             shared helpers: errors, attempt, SecureScreen, appViewModel
│   ├── onboarding/         Welcome, Backup, VerifyBackup, Restore
│   └── home/               Home
└── harness/                the original one-screen harness (removed in step 7)
```

Organized by feature: a new area gets its own `ui/<feature>/` package, one file
per screen, each holding the screen's ViewModel and composable.

## Steps

All in one PR, one step at a time, each tested on a device before the next.

| Step | Scope | SDK surface | Status |
|---|---|---|---|
| 1 | Compose, navigation, `WalletSession`; onboarding: create, back up, verify, restore | `createFedimintSdk`, `Mnemonic.fromWords`, `exportMnemonic().words()` | done |
| 2 | Home: live balance, capability-gated actions, connectivity | `balanceUpdates`, `capabilities`, `federationStatus` | |
| 3 | Federations: list, details, join/preview, reopen/close/forget, quarantine diagnostics | `storedFederations`, `federationStatusUpdates`, `preview`, `join`, `reopenFederation`, `closeFederation`, `forgetFederation`, `Diagnostic` | |
| 4 | Send and receive: Lightning, ecash, on-chain; quote, then approve, then execute | `lightning()`, `ecash()`, `onchain()`, `quote`/`send`/`receive` | |
| 5 | Activity: paginated history, operation detail with live state and cancel | `activity(cursor)`, `operation(id)`, `AnyOperation`, `updates()`, `requestCancel` | |
| 6 | Recovery progress and resume; federation metadata | `recover`, `recoveryStatus`, `resumeRecovery`, `meta`, `ConsensusMetadata` | |
| 7 | Remove the harness once every API above has a wallet screen | | |

## Testing step 1

Build and install (see `android/README.md` for producing the native library and
bindings first):

```sh
cd android && ./gradlew :app:installDebug
```

1. **Fresh install:** Welcome appears.
2. **Create:** 12 words appear; screenshots of this screen come out black.
   Continue stays disabled until the checkbox is ticked.
3. **Verify:** you're asked for 3 words at random positions. A wrong word says
   which one; the right words go to Home. Back from Home leaves the app.
4. **Relaunch:** goes straight to Home.
5. **Kill during backup:** clear data, create, close the app on the phrase screen,
   and relaunch. It returns to the phrase screen with the same words.
6. **Restore:** clear data (`adb shell pm clear org.fedimint.demo`), choose Restore,
   and enter the words. It goes to Home. A made-up phrase shows "That input isn't
   valid."
7. **Rotation** on any screen keeps what was typed.
8. **Developer tools** on Home opens the old harness, which still works on its own
   data.
