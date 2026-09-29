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

### 10. Federation state comes from the SDK's status stream, not per-screen polling

`WalletSession.federations` is one `StateFlow<List<FederationInfo>>`, seeded from
`storedFederations()` and then updated from `federationStatusUpdates()`: each update
replaces the row with the same id, and `Forgotten` removes it. Every screen that
lists or badges federations reads this single flow.

- **Why `storedFederations` and not `federations()`:** the SDK docs are explicit.
  `federations()` answers "what can I act on" and silently drops closed or
  quarantined federations. `storedFederations` answers "what does this user have",
  so a federation that can't be used right now shows up labelled instead of
  vanishing along with the user's view of their money.
- **Why one app-level subscription:** a status can change with nobody asking, for
  example guardians publish a config the SDK refuses and the federation is
  quarantined. One long-lived subscriber in the session catches that for every
  screen, rather than each screen opening its own.
- **Live data per screen (the balance):** the ViewModel follows `balanceUpdates()`,
  whose first `next()` is the current amount and each later one a change. It is
  keyed on *(federation id, is open)*, so it restarts when the user switches
  federation or the federation closes or reopens, and not on unrelated status
  changes. `SharingStarted.WhileSubscribed(5_000)` stops it 5 s after the screen
  leaves (for example the app goes to the background), but not during a rotation.
- **Capabilities gate the UI before the user taps:** Send and Receive are enabled
  only on a `Running` federation with at least one capability, and the method
  sheet lists only what `capabilities()` reports. A `Recovering` federation shows
  its provisional balance but keeps both disabled, because the SDK refuses every
  send and receive until recovery completes.
- **Which federation Home shows** is app state (`selected_federation` in
  `SharedPreferences`). If the selected federation was forgotten, Home falls back to
  the first running one (`WalletSession.pickActive`).

### 11. Federation lifecycle actions follow the status, and results come from the stream

The detail screen offers only what the SDK allows in the current status:

| Status | Actions |
|---|---|
| Running, Recovering | Show on Home, Close, Remove |
| Closed | Reopen, Remove |
| Quarantined | Reopen (retry), Close (stop retrying), Remove |
| Forgetting | Retry removal |

- **The screen never assumes an action's result.** After Close, Reopen or Remove
  it waits for the new status to arrive on `federationStatusUpdates()` (decision
  #10) and renders that. What's shown is always what the SDK reports, including
  the cases where an action lands somewhere unexpected (for example a failed
  reopen leaves the federation quarantined).
- **Close is safe, Remove is not, and the UI says so.** Both are confirmed.
  Close's dialog says the balance and history stay and it can be reopened. Remove's
  dialog says the history is deleted, and that it only works at zero balance with
  no payments in progress.
- **A refused Remove still closes the federation.** The SDK stops the federation
  before checking whether it may erase it, so a `BALANCE_NOT_EMPTY` or
  `PENDING_OPERATIONS` refusal leaves it Closed. The error message says exactly
  that and points to Reopen, so the user isn't left wondering why their federation
  stopped.
- **Removing during recovery gets its own warning.** It is the SDK's only way out
  of a recovery that can't finish, and it throws away everything recovered so far.
  The dialog spells that out before the user confirms.
- **Quarantine shows the SDK's `Diagnostic`:** the stable `ErrorCode` plus the
  message. The structured `details` envelope isn't rendered yet.

## Package layout

```
org.fedimint.demo
├── FedimintApp.kt          Application + AppContainer (manual DI)
├── MainActivity.kt         the single Compose activity
├── wallet/                 SDK ownership: WalletSession, and repositories as they come
├── ui/
│   ├── nav/                routes, NavHost, start-screen decision
│   ├── theme/              Material 3 theme
│   ├── common/             shared helpers: errors, attempt, SecureScreen, appViewModel,
│   │                       formatting, federation status labels and badge
│   ├── onboarding/         Welcome, Backup, VerifyBackup, Restore
│   ├── home/               Home: balance, status, send/receive entry
│   └── federations/        Federations list, FederationDetail, JoinFederation
└── harness/                the original one-screen harness (removed in step 7)
```

Organized by feature: a new area gets its own `ui/<feature>/` package, one file
per screen, each holding the screen's ViewModel and composable.

## Steps

All in one PR, one step at a time, each tested on a device before the next.
Preview and join moved from step 3 to step 2, because Home can't be tested
without a joined federation.

| Step | Scope | SDK surface | Status |
|---|---|---|---|
| 1 | Compose, navigation, `WalletSession`; onboarding: create, back up, verify, restore | `createFedimintSdk`, `Mnemonic.fromWords`, `exportMnemonic().words()` | done |
| 2 | Home: live balance, status, capability-gated actions, federation switcher; join a federation (preview, then join) | `storedFederations`, `federationStatusUpdates`, `balanceUpdates`, `capabilities`, `preview`, `join` | done |
| 3 | Federation manager: list, details, reopen/close/forget, quarantine diagnostics, copy invite code | `reopenFederation`, `closeFederation`, `forgetFederation`, `inviteCode`, `Diagnostic` | done |
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

## Testing step 2

1. **No federation:** Home shows "Join a federation" with a button.
2. **Join:** tap it, then "Use the Mutinynet test federation", then Preview. It
   shows mutinynet-05-alephbft on Signet with 4 guardians. Preview can take a
   minute on a loaded machine; a TIMEOUT error just means try again.
3. Tap Join. Home shows the federation name, a balance (0 sats), Signet, Connected,
   and a test-network note.
4. **Capabilities:** Receive and Send each open a sheet listing only the methods
   this federation supports (here Lightning, Ecash and On-chain; they're wired up in
   step 4).
5. **Live balance:** not testable yet. The harness under Developer tools has its
   own separate wallet, so it can't fund this one. The live update is checked in
   step 4, with the first real receive.
6. **Relaunch:** Home comes straight back to the same federation and balance.
7. **Join again** from the ⋮ menu with the same code: no error, it just selects it.
8. **Bad code:** type anything else and Preview. You get "That input isn't valid."

## Testing step 3

Join the Mutinynet federation first (step 2) if the wallet has none.

1. **List:** Home → ⋮ → Federations lists every federation with its network and a
   status badge. Join (the button at the bottom right) opens the join screen.
2. **Detail:** tap a federation to see its status, network and id, plus Copy invite
   code (paste it somewhere to check).
3. **Close:** confirm the dialog. The status becomes Closed, and the actions change
   to Reopen and Remove. Back on Home: balance "—", a "Closed on this device"
   note, and Send and Receive disabled.
4. **Reopen:** the status returns to Connected (this contacts the guardians, so it
   can take a while), and Home shows the balance again.
5. **Show on Home:** returns to Home with this federation selected. It's useful
   with two or more federations.
6. **Remove at zero balance:** confirm. You're taken back to the list, which is now
   empty, and Home shows its "Join a federation" state.
7. **Not testable yet:** a refused Remove (needs a balance, so step 4) and
   quarantine (needs a federation whose configuration the SDK rejects).
