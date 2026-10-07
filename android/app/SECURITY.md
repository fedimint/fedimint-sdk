# Sensitive data in the reference wallet

Required reading before changing how the wallet shows, stores or moves any of
the data below, or adding a screen that does. Each rule names where it is
enforced, so a change can keep it.

## What is sensitive

| Data            | Why                                                                          | Where it appears                                                          |
| --------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Recovery phrase | Whoever sees it owns every federation's funds                                | Backup, Verify, Restore, Recovery phrase screens                          |
| Ecash notes     | A bearer instrument: whoever holds them can redeem them                      | Send ecash output, Redeem ecash input, operation detail (unredeemed send) |
| Seed origin     | Decides whether joining recovers funds; getting it wrong loses them for good | `WalletSession.recordSeedOrigin`, `SeedOrigin`                            |

Invite codes are shareable by design and not treated as secrets.

## Rules

1. **Screens that show or take a recovery phrase call `SecureScreen()`**
   (`ui/common/Compose.kt`). It sets `FLAG_SECURE`: no screenshots, screen
   recordings or recent-apps thumbnail. The flag is counted per window
   (`SecureFlagCounter`), because screens overlap during navigation: Verify is
   shown before Backup is disposed. Never set or clear `FLAG_SECURE` directly;
   a direct clear from one screen exposes the next.
2. **Secrets never go into saved state.** No `rememberSaveable`, no
   `SavedStateHandle`, no navigation arguments: Android may write those to disk.
   The typed phrase and pasted notes live in ViewModel memory only
   (`RestoreViewModel`, `EcashReceiveViewModel`).
3. **Secrets never go into logs.** Failures log the SDK's `ErrorCode` and
   `reason()` (`PaymentViewModel`), which carry no secrets. Don't log inputs,
   `Notes.display()` or `Mnemonic.words()`.
4. **Copying a phrase goes through `copySecret()`** (`ui/common/Clipboard.kt`):
   after a warning, flagged `EXTRA_IS_SENSITIVE`, and cleared a minute later if it
   is still the clipboard's content. Android lets only the focused app read the
   clipboard, so if the wallet is in the background at that point the check runs
   again when it regains focus (`MainActivity.onWindowFocusChanged`). The UI says
   so rather than promising a hard 60 seconds. If Android ends the process first,
   nothing clears it.
5. **The seed's origin is written before the seed exists,** synchronously
   (`commit()`), and an unknown origin counts as "may hold funds"
   (`SeedOrigin.recoverByDefault`). Otherwise a crash between restoring a seed
   and recording it would offer a plain join, which can never be recovered.
6. **No backup of app data** (`android:allowBackup="false"`): the wallet's
   storage holds the seed.
7. **Money moves once.** An operation is recorded the moment the SDK returns it,
   and a failed send drops its consumed quote (`PaymentViewModel.execute`). Never
   show a send control for an operation that already exists.

## The debug harness

`app/src/debug/.../harness/HarnessActivity.kt` (debug builds only, for the
Appium suite) shows the seed without `FLAG_SECURE` and keeps its own wallet in
`files/harness`. It must never be moved into `src/main`.
