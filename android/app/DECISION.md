# Android reference wallet: design decisions

Tracking issue: [#399](https://github.com/fedimint/fedimint-sdk/issues/399).

`android/app` is a reference wallet: an app someone building on the Fedimint
Android SDK can read and copy. This file records the decisions behind it, and the
alternatives turned down, so they don't have to be re-argued.

**Before changing anything that shows, stores or moves the recovery phrase, ecash
notes or the seed's origin, read [SECURITY.md](SECURITY.md).**

## Constraints

- **One SDK instance per storage location.** `createFedimintSdk` claims its data
  directory, so whatever owns the wallet's SDK must outlive screens and rotation.
- **The SDK is async and streaming:** `suspend fun`s, plus subscriptions
  (`balanceUpdates`, `federationStatusUpdates`, `updates()`) polled with `next()`.
- **CI compiles this app to prove the bindings are usable** (`android-sdk.yaml`).
- **The toolchain is pinned** (Kotlin 2.0.21, AGP 8.13, compileSdk 36).

## Architecture

- **Jetpack Compose**, not XML Views: screens are functions of state, which suits
  the SDK's streams. Views + Fragments would be about twice the code per screen.
- **One activity (`WalletActivity`), one Navigation Compose graph, type-safe
  routes** (`ui/nav/Routes.kt`): screen arguments are checked at compile time.
- **Three tabs (Home, Activity, Federations)** with Home always the root; every
  other screen is pushed over them, full screen, as the shared design says for
  Android.
- **One app-scoped `WalletSession` owns the `Sdk`** (forced by the lock) and makes
  every call main-safe. `Payments` (module facades) and `History` sit beside it.
- **A ViewModel per screen, exposing one `StateFlow<UiState>`.** One-off outcomes
  are `UiState` fields acted on with `LaunchedEffect`, so none is lost on rotation.
- **Manual DI** (`AppContainer`, `appViewModel { }`), not Hilt: the graph is a few
  objects, and a reference app should show the SDK, not a DI framework.
- **Library versions are the newest built for Kotlin 2.0** (Compose BOM 2024.12.01,
  Navigation 2.8.5, Lifecycle 2.8.4), so the pinned toolchain needs no bump. QR
  codes use ZXing core (plain Java).
- **Package by feature:** `ui/<feature>/`, one file per screen holding its
  ViewModel and composable.

## Look

- **The shared demo design** (one Figma file for the iOS, Android and web
  demos) sets the screens, copy, tokens and components. Its tokens are mapped
  onto Material 3 roles (`ui/theme/Theme.kt`), so screens keep using
  `MaterialTheme`; its components are built once (`ui/common/Design.kt`).
- **Geist and Geist Mono are bundled** (OFL), as the design asks on every
  platform. Icons are the design's Lucide set as vector drawables; the brand
  mark and logo are the design's copies of the official SVGs.
- **The design is light only.** Dark mode keeps Material's baseline dark scheme
  until the design has dark tokens.
- **Where the design shows data the SDK doesn't report** (a fiat value,
  guardians online, recovery per module, ecash spent-ness, a confirmation
  count), it is left out rather than invented.

## The SDK harness

The old one-screen page that calls every SDK export lives on as
`harness/HarnessActivity.kt`, in **debug builds only** (`app/src/debug`). Upstream's
Appium suite (`js/android/integration-tests`) drives it by view id, and now
launches it instead of the wallet. Release builds contain only the wallet. It keeps
its own data directory (`files/harness`), so it never touches the wallet's seed.
`DebugApplication` extends `FedimintApp`, and AppCompat and Material Components are
`debugImplementation` only. Porting the suite to the wallet's screens is a follow-up.

## Onboarding

- **Start screen:** no wallet → Welcome; wallet with an unconfirmed backup →
  Backup; otherwise Home. "A wallet exists" means the SDK's data directory isn't
  empty, as the SDK has no lighter check.
- **Backup is confirmed** by picking the word at 3 random positions, each from
  three words of the same phrase.
- **The seed's origin (created or restored) is recorded before the seed exists**,
  and an unknown origin is treated as restored. It decides whether joining a
  federation leads with recovery (see Recovery).
- **The phrase can be viewed again** (hidden until Reveal) **and copied**, with the
  clipboard precautions in SECURITY.md.

## Federations

- **The federation list is one app-wide stream:** seeded from `storedFederations()`
  and kept current by `federationStatusUpdates()`. `storedFederations`, unlike
  `federations()`, keeps closed and quarantined federations visible, labelled.
- **Home never shows one federation's data under another's name:** each result is
  tagged with the federation it was read for, and a switch resets to loading.
- **Capabilities gate Send and Receive** before the user taps; a recovering
  federation shows its balance but keeps both disabled.
- **Lifecycle actions follow the status** (Running: close, remove; Closed or
  Quarantined: reopen, remove; Forgetting: retry), and the screen renders the status
  the SDK reports afterwards, never an assumed result.
- **Close is safe, Forget is not:** both are confirmed. A refused Forget
  (`BALANCE_NOT_EMPTY`, `PENDING_OPERATIONS`) still leaves the federation closed,
  and the message says so and points to Reopen.

## Payments

- **Send and Receive are one screen each, with a tab per rail the federation
  runs** (unsupported rails are hidden, not disabled). Each rail keeps its own
  ViewModel for the life of the screen.
- **Sends are quote → review → send.** The review shows amount, fee, total, the
  Lightning route and gateway, and a countdown to the quote's expiry. A quote is
  single use; editing the input or a failed send drops it, so Review fetches a
  fresh one.
- **Money moves once.** An operation is recorded the moment the SDK returns it,
  which locks the screen. A submission in progress is also recorded in saved state
  _before_ the call, so a screen restored after process death stays locked and
  reconciles against the SDK's history instead of offering the payment again.
- **Receives hand something over** (invoice, notes) as a QR and copy button, then
  follow the operation's `updates()`.
- **On-chain deposits** (since upstream #453): `receive()` returns only an address;
  `deposits()`, opened before the address is shown, hands over each deposit once
  it's confirmed. An address isn't a submission, so that screen needs no
  reconciliation.
- **Each state's wording lives in one place** (`OperationStates.kt`), exhaustive
  over the SDK's types, so a new SDK state breaks the build rather than showing
  nothing.
- **Handles are closed with the screen**; the SDK keeps running the operation.

## Activity

- **History is the SDK's own log, paged by cursor** ("Load more"), with no
  app-side copy, grouped by day. The rail filter applies to the rows loaded so
  far, since the SDK can't filter a page. Loads
  carry a generation, so a refresh during paging can't duplicate rows.
- **Home's recent activity reloads** on balance changes, when Home comes back into
  view, and every 10 s while a row is pending (settling doesn't always move the
  balance).
- **An operation is narrowed only after `support()`:** one written by a newer SDK is
  shown as "Unrecognised operation", not as an error.
- **Unredeemed ecash can be reclaimed** (`requestCancel()`), after a confirmation
  that the receiver wins if they redeem first.

## Recovery and federation info

- **A restored wallet joins by recovering:** a plain `join()` can never be turned
  into a recovery later, so "Join and recover funds" leads unless the seed is known
  to be created here.
- **The recovery screen reattaches** to a running recovery (`resumeRecovery()`), but
  retries a failed one only when the user asks, since that starts a new attempt.
- **Progress** is the SDK's `RecoveryProgress` as one bar (it isn't reported per
  module), plus elapsed time; the balance is described as incomplete until Done.
- **Federation info** shows `meta()`'s three views read-only: merged, configuration,
  and the consensus document.
