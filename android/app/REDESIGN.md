# Reference wallet: the shared demo design

How `android/app` implements the shared demo design for the Fedimint SDK demos
(Figma: _Fedimint SDK – Unified Demo Wallet_, 17 screens plus a components
sheet). Design decisions that outlive this change are recorded in
[DECISION.md](DECISION.md); the security rules are in [SECURITY.md](SECURITY.md).

## What the design asks for

The design's own rules page sums it up:

- 17 screens are the single source of truth for the iOS, Android and web demos:
  the same screens, order, copy, tokens and components.
- Three tabs: Home, Activity, Federations. Send and Receive open as full screens
  on Android.
- Send and Receive are one screen each, with a Lightning / Ecash / Onchain
  control. A rail the federation doesn't run is hidden, never disabled.
- Geist and Geist Mono on every platform, and only the official Fedimint SVGs.
- Allowed to be native: the status bar, the back gesture, haptics, the share
  sheet and the camera scanner.

## How it is built

### Tokens: `ui/theme/Theme.kt`

The Figma variables are mapped onto Material 3 roles, so screens keep reading
`MaterialTheme.colorScheme` and `MaterialTheme.typography` and need no
theme-specific code.

| Design token                                      | Material role                                                   |
| ------------------------------------------------- | --------------------------------------------------------------- |
| `color/bg` `#F6F6F2`                              | `background`, `surface`                                         |
| `color/surface` white                             | `surfaceContainer*` (cards, fields, buttons)                    |
| `color/surface-2` `#EEEEE8`                       | `surfaceVariant` (icon tiles, segmented control)                |
| `color/border` `#E4E4DC`                          | `outline`                                                       |
| `color/text` / `text-muted`                       | `onSurface` / `onSurfaceVariant`                                |
| `color/primary` `#181884` / `primary-soft`        | `primary` / `primaryContainer`                                  |
| `color/danger` / `danger-soft`                    | `error` / `errorContainer`                                      |
| `success`, `warning`, `hero`, `on-hero`, `accent` | `WalletColors` (`MaterialTheme.walletColors`): no Material role |

The text styles map the same way: Title/Large to `headlineLarge`, Title/Medium
to `titleLarge`, Body/Strong to `titleMedium` and `labelLarge`, Body/Small to
`bodyMedium`, Display/Balance to `displaySmall`. Overline, Mono/Body and
Mono/Amount have no Material slot and live in `WalletText`. The corner radii
are 8, 12 and 16 dp.

The design is light only. Dark mode keeps Material's baseline dark scheme for
now (see Open questions).

### Components: `ui/common/Design.kt`

Each component on the design's components sheet is built once:

- `WalletButton` in the six styles: Primary, Secondary, Soft, Danger, Inverse
  and Outline-on-dark. It is 52 dp tall, with an optional icon and a busy
  state.
- `StatusPill`, with Success, Warning, Neutral and Danger tones.
- `WalletCard`, `DetailRow` / `DetailCard`, `CardDivider`.
- `IconTile`, `FederationAvatar`.
- `SegmentedControl`, `ChoicePill` (filters, 12/24 words) and `ChoiceTile`
  (backup check).
- `AmountHero` (the big `21,500 sats`), `AmountInput` (the same, editable),
  `WalletTextField`.
- `NavHeader`, `TabTitle`, `Overline`, `SectionHeader`, `ProgressBar`,
  `TimelineStep`.
- `WalletScreen`: a header, a scrolling body, and actions pinned to the bottom
  that ride above the keyboard. Most screens are built from it.

QR codes, the share sheet and the camera scanner are in `ui/common/Qr.kt`.

### Assets

| Asset                                                        | Where           | Source and licence                                                                                                                                                                                                                                                               |
| ------------------------------------------------------------ | --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Geist Regular/Medium/SemiBold, Geist Mono Regular/Medium     | `res/font/`     | `geist@1.3.1` npm package; SIL OFL 1.1, text in [GEIST_LICENSE.txt](src/main/assets/licenses/GEIST_LICENSE.txt), shipped in the APK                                                                                                                                              |
| 21 line icons (`ic_*.xml`)                                   | `res/drawable/` | The design's icon set, which is Lucide (ISC); their geometry matches lucide-static 0.396.0 to 0.510.0. Converted from the design's SVGs to vector drawables. Lucide and Feather notices in [LUCIDE_LICENSE.txt](src/main/assets/licenses/LUCIDE_LICENSE.txt), shipped in the APK |
| Fedimint mark, mark mono, logo, logo on dark (`brand_*.xml`) | `res/drawable/` | Converted from the design's SVGs, which it labels "official asset, do not redraw"                                                                                                                                                                                                |

The Welcome screen's large decorative mark is the same `brand_mark` drawable,
scaled. No new image was drawn.

## Screen by screen

| #   | Design screen     | Android                                                                                     | Notes                                                                                                                                                                                                                                 |
| --- | ----------------- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 01  | Welcome           | `onboarding/Welcome.kt`                                                                     | The design's dark screen. Status bar icons turn light while it shows.                                                                                                                                                                 |
| 02  | Back up seed      | `onboarding/Backup.kt`, `SeedGrid.kt`                                                       | Words in a numbered 2-column grid, row by row as in the design. Keeps "Copy phrase" (with its warning) and, when opened from Home, the Reveal step.                                                                                   |
| 03  | Verify backup     | `onboarding/VerifyBackup.kt`                                                                | Now picks each of 3 words from three choices (the others come from the same phrase) instead of typing them. A wrong pick clears that word and names it.                                                                               |
| 04  | Home              | `home/Home.kt`                                                                              | Federation switcher with the brand mark; scan in the corner. Shows the balance, then the network and the modules the federation runs, then Receive (primary) and Send, then recent activity. A shield icon opens the recovery phrase. |
| 05  | Receive Lightning | `payments/Lightning.kt` (`LightningReceivePane`)                                            | Big amount input and a note, then the QR, a status pill and Share invoice.                                                                                                                                                            |
| 06  | Review quote      | the same Send screen, in review mode                                                        | "You're sending" with the amount large, then To, Via, Gateway and fees, the total, and "Quote locked · expires in m:ss". Approve & send / Cancel. Back cancels the review.                                                            |
| 07  | Federations       | `federations/Federations.kt` (tab)                                                          | Active and Inactive groups, with avatar, subtitle and pill. Join with invite code is at the bottom, and as + in the title.                                                                                                            |
| 08  | Activity          | `activity/Activity.kt` (tab)                                                                | All / Lightning / Ecash / Onchain chips, rows grouped by day (Today, Yesterday, date), and Load more. Pull to refresh is kept.                                                                                                        |
| 09  | Operation detail  | `activity/OperationDetail.kt`                                                               | Icon, kind, signed amount and status pill; a detail card with copy buttons; Progress as a timeline. For ecash: Show notes again and Reclaim notes.                                                                                    |
| 10  | Recovery          | `federations/Recovery.kt`                                                                   | Big percent, "Recovering your funds", one scan bar, balance so far, time running. Go to Home, plus Retry recovery after a failure.                                                                                                    |
| 11  | Restore           | `onboarding/Restore.kt`                                                                     | A 12/24 switch and a box per word; a space moves to the next box. Paste phrase fills the boxes, switching to 24 if needed.                                                                                                            |
| 12  | Join              | `federations/JoinFederation.kt`                                                             | Large title, invite field, Scan invite QR, the Mutinynet shortcut, and a preview card. Join and recover / Join without recovering, as before.                                                                                         |
| 13  | Federation detail | `federations/FederationDetail.kt`                                                           | Avatar, name and pill; the diagnostics card when quarantined; an info card (ID, network, balance, invite code). Show on Home or Reopen, then Close and Forget.                                                                        |
| 14  | Send              | `payments/PaymentScreens.kt` (`SendScreen`) and the Lightning, Ecash and Onchain send panes | Invoice or address field with Paste and Scan, big amount input, "Available: … sats".                                                                                                                                                  |
| 15  | Receive Ecash     | `payments/Ecash.kt` (`EcashReceivePane`)                                                    | Paste or scan notes. The preview (amount and federation) appears on its own shortly after the text stops changing. "Redeem N sats".                                                                                                   |
| 16  | Receive Onchain   | `payments/Onchain.kt` (`OnchainReceivePane`)                                                | QR, a card with the network, confirmations and deposit fee, the deposit's progress, then Share address and New address.                                                                                                               |
| 17  | Ecash ready       | the same Send screen, after Create notes                                                    | Amount, "From <federation>", the QR of the notes, "Waiting for recipient to redeem", then Share notes and Reclaim notes (confirmed first).                                                                                            |

### Navigation (`ui/nav/WalletApp.kt`, `Routes.kt`)

- **Tabs.** A `Scaffold` shows the tab bar only on the three tab destinations.
  Home is always the root: switching tabs pops back to Home first, so Back from
  Activity or Federations returns to Home.
- **Routes.** The six per-rail payment routes became `Send(federationId)` and
  `Receive(federationId)`. `Activity(federationId)` became `ActivityTab`, which
  follows the federation shown on Home.
- **Payment screen frame.** `PaymentFrame` draws the header and the rail tabs.
  Each rail is a "pane" with its own ViewModel, so switching tabs keeps what
  was typed or created. A pane tells the frame its title, whether the tabs show
  and what Back does (`PaneChrome`); that is how the review and "Ecash ready"
  states hide the tabs and retitle the screen.

### Behaviour that was added

- **QR scanning.** The scanner uses CameraX for the camera and decodes with
  ZXing, which the app already used to draw codes, so it needs no Google Play
  services. Scan appears on Home, in each code field, and on Join.
  - What was scanned is judged by its form (`Scanned.classify`): an invoice or
    address opens Send, notes open Receive, an invite opens Join.
  - It reaches that screen through `ScanHandoff`, in memory and taken once,
    never as a navigation argument (SECURITY.md, rule 2).
  - The camera permission is asked only when Scan is tapped.
- **Share.** Invoices, addresses and ecash notes can be sent through the system
  share sheet.
- **Reclaim on the Ecash-ready screen.** `EcashSendViewModel.reclaim()` calls
  `requestCancel()` on the operation it created. Before, reclaiming was only
  possible from Activity.
- **Progress timeline.** `PaymentViewModel` now records each state it sees, and
  when (`UiState.steps`).

### What did not change

ViewModels, the SDK calls and their safety rules are unchanged:

- quote, then review, then send;
- an operation is recorded the moment the SDK returns it;
- a mid-submission restore is reconciled against history;
- `FLAG_SECURE` on every phrase screen;
- `copySecret` for the phrase;
- secrets kept out of saved state.

The few ViewModel changes are additive:

- `Review.amount` (the headline figure);
- `UiState.steps`;
- `EcashSendViewModel.canReclaim` and `reclaim()`;
- `VerifyBackupViewModel` options and picks;
- `RestoreViewModel` holds a word list instead of one string;
- `FederationDetailViewModel.balance`.

## Where it differs from the design, and why

| Design shows                                                  | The app shows                                   | Why                                                                                                         |
| ------------------------------------------------------------- | ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| `≈ $14.20 USD` under amounts                                  | nothing                                         | The SDK has no price data. A reference wallet would need an outside price service.                          |
| `4/4 guardians online` on Home                                | the network and the modules the federation runs | The SDK doesn't report guardian connectivity.                                                               |
| Pill "Online" for a running federation                        | "Open"                                          | `Running` is the client's lifecycle, not connectivity; "Online" would claim something the app doesn't know. |
| Recovery split by module (Mint 62%, Lightning Done, …)        | one "Federation history scan" bar               | `RecoveryProgress` is a single complete/total.                                                              |
| Ecash preview "Valid · unspent"                               | "Readable · checked on redeem"                  | `Notes` exposes only its value; whether notes are spent is known only on redeem.                            |
| "Invoice or lightning address"; amount shown from the invoice | invoice only; the amount shows on the quote     | The SDK has no Lightning address support or invoice decoder.                                                |
| `Confirmations: 6 required`                                   | "As the federation requires"                    | The number isn't exposed.                                                                                   |
| Progress timeline with times                                  | the states this screen saw, with times          | The SDK doesn't record when each state began; a screen opened later starts from the current state.          |
| Payment detail "Request cancel"                               | "Reclaim notes" for unredeemed ecash only       | Only ecash sends can be cancelled through the SDK.                                                          |
| No recovery-phrase entry on Home                              | a shield icon next to scan                      | Seeing the phrase again is a required wallet feature, and the design has no Settings tab.                   |
| Backup step 1 has no copy                                     | "Copy phrase", with its warning                 | Requested earlier in #430; it follows SECURITY.md, rule 4.                                                  |

## Testing

- **Build and unit tests.** `./gradlew :app:testDebugUnitTest :app:assembleDebug`
  passes, with the JVM tests CI now runs too. They include the review fixes:
  - history reconciliation (a match on page two, an earlier payment of the
    same kind, an unclear result staying locked);
  - deposits followed one at a time;
  - a failed redeem checked again.
- **Checked on the emulator** (Pixel 3a, API 34, Mutinynet, with the native
  library from `main`):
  - **Onboarding and federations:** Welcome, Restore (typing a phrase),
    Join with recovery, Recovery to completion (1,000 test sats back), Home,
    Federations, Federation detail, and Recovery phrase (Reveal).
  - **Lightning:** Receive (21-sat invoice, QR, waiting pill); switching rail
    tabs and back keeps the invoice; Send (paste, Review quote, Approve & send,
    Paid with its timeline).
  - **Onchain:** Receive (address, New address).
  - **Ecash:** Send (Review, Create notes, the Ecash-ready screen); Receive
    (paste, automatic preview, Redeem); Reclaim notes (confirm, Reclaimed).
  - **Errors:** a send over the balance shows "Not enough balance".
  - **Activity:** filter chips, day groups, Payment detail.
  - **Scanner:** opens, the camera runs, and Close returns without navigating.
- **Screens that block screenshots:** Restore and Recovery phrase do, as they
  should (`FLAG_SECURE`), so they were checked through the accessibility tree.
- **Not checked:**
  - Scanning a real QR code with a physical camera.
  - The share sheet.
  - Paying a Lightning invoice through an external gateway; only the internal
    route was tested.

## Open questions for the maintainers and designer

1. Dark-mode tokens for the design.
2. Confirmation that the brand SVGs in the Figma file are the official ones,
   since `js/docs/public` no longer exists in the repo.
3. SDK support for the items in "Where it differs": guardian connectivity,
   recovery per module, ecash spent-ness, Lightning addresses and invoice
   decoding, the required confirmations, and a kind filter on history pages.
