# @fedimint/integration-tests-android

Android device-level tests for the fedimint SDK, driven via [Appium](https://appium.io/)
against the example app in [`android/app`](../../../android).

**This tests the SDK, not the example app.** `android/app` is one scrolling screen that calls
every export of `rust/fedimint-sdk`'s `uniffi` feature through the generated Kotlin bindings
— it has no product surface of its own. What these tests add over `android-sdk.yaml`, which
compiles the same app, is a running device: the bindings are loaded, the native `.so` is
mapped, and the calls actually execute. Tests here are organized by SDK capability, mirroring
the naming in `js/web/integration-tests/src/services/*.test.ts` (the WASM/browser
equivalent), not by UI flow.

Why Appium and not Detox/Maestro: see the "Android E2E (Appium)" section of
[`docs/core/dev/testing.md`](../../docs/core/dev/testing.md).

## Running

Enter the `android-tests` Nix devshell first — it provides the Android SDK, NDK, emulator, and
a system image (extends the plain `android` shell used for building the FFI crate, kept
separate so that shell doesn't pay for the emulator's multi-gigabyte closure), and wires
`PATH`/`APPIUM_HOME` around the pnpm-installed `appium` binary (a plain npm devDependency of
this package — Nix supplies the Android toolchain around Appium, not Appium itself):

```bash
just test-android-e2e        # a v1 federation (mint, wallet, ln)
just test-android-e2e v2     # a v2 federation (mintv2, walletv2, lnv2)
```

That builds the example APK, then boots (or lets you pick) a device, starts Appium, and runs
the suite inside a devimint federation. There is one way to run it on purpose: no
federation-free variant, so what CI runs and what you can reproduce are the same thing.

### Building and running are separate steps

`just test-android-e2e` is two recipes: `just build-android-apk`, then the run. The script
that drives the device, `run-android-e2e.sh`, **builds nothing** — it installs a finished APK
from where Gradle leaves it (`android/app/build/outputs/apk/debug/`) and stops with a message
if there isn't one.

That is a resource decision, not tidiness. The run happens inside a devimint federation
(bitcoind, four guardians, two gateways, LND, LDK, esplora) on a machine that is also booting
an emulator. A cold Gradle build on top of that starved the emulator until Android's own
System UI stopped responding — every test then failed against the "System UI isn't responding"
dialog rather than against the app. So the build finishes, daemon and all, before either of
those starts.

CI makes the same split across machines, as separate jobs in `android-sdk.yaml`. Each task runs
exactly once and the next job downloads its output rather than redoing it:

| job        | workflow                            | shell             | produces                                                  |
| ---------- | ----------------------------------- | ----------------- | --------------------------------------------------------- |
| `native`   | `android-native.yaml` (self-hosted) | —                 | `jniLibs`, the cross-compiled `.so`                       |
| `bindings` | `android-sdk.yaml`                  | — (Rust)          | `android-bindings`, generated once from that `.so`        |
| `apk`      | `android-apk.yaml`                  | `.#android`       | `android-example-apk` — Gradle on the two artifacts above |
| `e2e`      | `android-e2e.yml`                   | `.#android-tests` | the run itself — installs the APK, no Gradle              |

The AAR job (`aar`) downloads the same `jniLibs` and `android-bindings` and only assembles the
release AAR; the example app is compiled once, by `apk`.

By hand, the two halves:

```bash
just build-android-apk                       # `.#android`: native lib, Kotlin, Gradle

nix develop .#android-tests
pnpm --dir js install                        # first time only
bash scripts/e2e-android/setup-and-start-appium.sh   # one-time per shell: installs/starts Appium
FM_SDK_SHAPE=v2 bash scripts/setup_test_shell.sh bash scripts/e2e-android/run-android-e2e.sh
```

`TESTS_TO_RUN` picks the tests (space-separated names from the table below, or `all`); without
it, the script asks. For example, just the on-chain round trip against v2:

```bash
FM_SDK_SHAPE=v2 TESTS_TO_RUN=onchain \
  bash scripts/setup_test_shell.sh bash scripts/e2e-android/run-android-e2e.sh
```

Or drive the runner directly once Appium is running and a device is configured:

```bash
PLATFORM=android \
AVD=<avd-name> \
BUNDLE_PATH=android/app/build/outputs/apk/debug/app-debug.apk \
APP_PACKAGE=org.fedimint.demo \
APP_ACTIVITY=org.fedimint.demo.harness.HarnessActivity \
FM_SDK_SHAPE=v1 \
ts-node --project tsconfig.json src/runner.ts mnemonic
```

Pass `all` instead of a test name to run every registered test. An unknown name fails the run
rather than being skipped. A federation-backed test driven this way needs the variables devimint
exports in its environment (see below), and `FM_SDK_SHAPE` set to the federation's real shape.

## Federation shapes

The federation runs one generation of the mint, wallet and lightning modules. `FM_SDK_SHAPE`
picks it, and `scripts/setup_test_shell.sh` turns it into the `FM_ENABLE_MODULE_*` flags devimint
starts fedimintd with (through `scripts/devimint-modules.sh`, which the Rust integration tests'
`scripts/devimint-shape.sh` shares, so a shape means the same modules everywhere). It defaults to
`v1`, which is what the wasm suite runs through the same script.

| shape   | modules                      | Android E2E | CI (`android-e2e.yml` matrix) |
| ------- | ---------------------------- | ----------- | ----------------------------- |
| `v1`    | `mint`, `wallet`, `ln`       | supported   | yes                           |
| `v2`    | `mintv2`, `walletv2`, `lnv2` | supported   | yes                           |
| `mixed` | v1 mint/wallet, `ln`+`lnv2`  | refused     | no                            |

`mixed` is refused by `run-android-e2e.sh` up front: the SDK rejects a federation that mixes
module generations by design, so there is no successful payment to test there.

Capabilities read the same on both generations, so `federation` checks the module kinds
`preview` reports against the shape, and fails if any of the other generation's are present.

## The tests

Listed in the order `all` runs them; each also runs on its own, with the fixtures in
`src/fixtures/` building the state it needs from a fresh install.

| name               | what it proves                                                                                                                                                                                                                                                                                               |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `mnemonic`         | the SDK opens over app storage and exports a well-formed mnemonic (no federation)                                                                                                                                                                                                                            |
| `inviteCode`       | `InviteCode.parse` and its federation id (no federation)                                                                                                                                                                                                                                                     |
| `federation`       | the join: regtest, all three capabilities, the module kinds of this shape and none of the other, an empty balance                                                                                                                                                                                            |
| `lightning`        | lightning in: the faucet pays an app invoice and the live balance rises by at most the invoice                                                                                                                                                                                                               |
| `mint`             | ecash out and back in: the debit is exactly the quoted notes + fee, the notes redeem (`Done`), and the send's own record is `ECASH_SEND` and not `CANCELED`                                                                                                                                                  |
| `lightningSend`    | lightning out: quote → pay a faucet invoice; the payee's node reports it paid, the balance drops by exactly the quoted total, and on v2 the preimage hashes to the invoice's payment hash                                                                                                                    |
| `errors`           | `INVALID_INPUT` for bad notes, a bad invoice and a zero amount; `INSUFFICIENT_BALANCE` quoting an invoice over the balance, with Pay left disabled and no balance or history change                                                                                                                          |
| `persistence`      | restart without clearing data: the same seed, the same federation (reattached on open), the same balance                                                                                                                                                                                                     |
| `operationRestart` | an unpaid receive survives a restart: found again by id through `federation.operation`, still waiting, then followed to `Claimed` once paid                                                                                                                                                                  |
| `onchain`          | deposit: bitcoind pays a fresh address, blocks are mined, the claim reports the txid, gross and net credit, and the balance rises by exactly the net credit; withdrawal: quote → send, `Succeeded(txid)`, the coins arrive and confirm at the destination, and the balance drops by exactly the quoted total |

### Where the generations differ

The tests assert what each generation documents rather than identical behaviour:

- **Fees and amounts.** lnv2 and mintv2 charge per transaction, so received amounts are bounded,
  not pinned, and every debit is checked against the SDK's own quote (`total`, or notes + fee).
  mintv2 rounds ecash up to a multiple of 512 msat, and this SDK only sends notes it already
  holds in exact denominations (`NOT_SUPPORTED` otherwise), so `mint` walks a short,
  generation-specific list of amounts and only moves on after `NOT_SUPPORTED`.
- **Ecash reclaim.** v1 reclaims an unredeemed send from a background timer (a day away); mintv2
  reclaims only when the send is observed after its deadline. Neither has to report a redeemed
  send as `REDEEMED` straight away, so `mint` requires only that it is not `CANCELED` (the generated
  Kotlin enum constant the example app prints).
- **Lightning send outcome on v1.** The v1 client cannot decode the success state
  (fedimint/fedimint#8969), so `awaitFinal` ends in `INTERNAL` mentioning the preimage. `lightningSend`
  accepts exactly that, and only on v1, and still requires the payee to report the invoice
  paid and the exact debit.
- **Deposit states.** v1 reports `WaitingForConfirmation`; walletv2 goes from waiting for a
  transaction to `Claimed`. `onchain` requires the former only on v1.
- **lnv2 gateway.** The lnv2 client picks a gateway at random, and only devimint's LND gateway is
  funded, so on v2 `run-android-e2e.sh` removes the other gateways from every guardian's lnv2
  list before the tests run. On v1 the SDK picks the cheapest gateway, which is LND (devimint sets
  its fees to zero). Either way the faucet's LDK node is the counterparty.
- **DNS check.** The fallback-to-Google-DNS warning fails every run. The "Got DNS servers" trace
  is required only on v1, where the lightning tests dial devimint's iroh gateway; on v2 they use
  the HTTP LND gateway and build no resolver.

### What the tests use from devimint

Everything comes from what `devimint wasm-test-setup` exports into the run:

| variable                                                     | used for                                                      |
| ------------------------------------------------------------ | ------------------------------------------------------------- |
| `FAUCET` (from `FM_PORT_FAUCET`)                             | invite code, paying app invoices, issuing invoices to pay     |
| `FM_LNCLI`                                                   | paying an invoice the faucet's node issued; decoding invoices |
| `FM_GWCLI_LDK`                                               | asking the payee node whether an invoice was paid             |
| `FM_BTC_CLIENT`                                              | funding deposit addresses, mining, checking withdrawals       |
| `FM_MINT_CLIENT`                                             | (script) pinning the lnv2 gateway list on v2                  |
| `FM_FEDERATION_BASE_PORT`, `FM_PORT_GW_*`, `FM_PORT_ESPLORA` | (script) `adb reverse` into the emulator                      |

A missing variable fails the test that needs it, naming it.

### Out of scope here

Follow-ups, not part of this suite: recovery and backup beyond what the SDK's own tests cover,
injecting protocol failures (rejected funding, gateway faults), succeeding against a mixed
federation, and a matrix of devices or API levels (CI runs one `android-34` emulator per shape).

## The federation

`scripts/setup_test_shell.sh` is the same entry point the wasm suite uses
(`js/package.json`'s `test:setup`): it execs the run inside
`devimint wasm-test-setup`, which stands up bitcoind, four guardians and two
gateways, and exports their ports. Tests reach the faucet through
`src/faucet/FaucetClient.ts` over `FAUCET`, exactly as
`js/web/integration-tests/src/test/TestingService.ts` does.

The app reaching that federation is the part with a twist. devimint binds
everything to `127.0.0.1` on the host and the invite code carries those URLs
verbatim (`ws://127.0.0.1:<port>`), but inside the emulator `127.0.0.1` is the
emulator. Rewriting the host to `10.0.2.2` is not an option — the URLs are
sealed inside a bech32m invite code the app parses — so
`run-android-e2e.sh` runs `adb reverse` over devimint's port window instead,
and the app joins through the unmodified code like any other client. The
gateways, the faucet and esplora (which the wallet client watches deposit
addresses through) are forwarded the same way; a test that needs another
service has to add its port to `reverse_devimint_ports`.

## Naming a view

`clickElementByKey`/`getTextByKey`/`typeIntoElementByKey` take the bare id a view carries in
[`activity_main.xml`](../../../android/app/src/main/res/layout/activity_main.xml) — `openWallet`,
`walletResult`, `seed`. `AppiumTestBase` qualifies it with `APP_PACKAGE` into the
`org.fedimint.demo:id/openWallet` resource-id that UiAutomator2 matches on, so a test never
repeats the package. Prefer these over `clickOnText`/`isTextPresent`: an id is stable across
copy changes, and several sections of the example app share button labels.

Every section writes `working…` into its result line before the SDK call and overwrites it
with the outcome, so assert on those with `waitForTextInElement(key, expected)` rather than
reading the text once.

## Adding a new SDK-service test

1. Add `src/services/<Name>Service.test.ts` — a class extending `AppiumTestBase` with a
   single `execute()` method that throws to fail. No Jest matchers; see
   `MnemonicService.test.ts` for the shape.
2. Register it in `src/registry.ts`'s `availableTests` map.
3. Declare `static produces` if the test leaves the app in a state a later test would
   inherit (an open SDK, a joined federation). The runner resets the app to a fresh install
   before any test whose `static prerequisites` don't include what is on the device. Scroll
   position is not part of that state: the runner scrolls back to the top before every test,
   so a test may leave the app scrolled anywhere.
4. If the test needs a starting state beyond a fresh install (e.g. an already-joined
   federation), add a fixture under `src/fixtures/` (see `src/fixtures/types.ts`) and declare
   `static prerequisites` on the test class — the runner resolves and caches fixtures across
   adjacent tests that share the same prerequisites.
5. If the test needs a real federation, declare `walletOpen` and `joinedFederation` (and
   `funded` if it spends) in `static prerequisites`; the fixtures in `src/fixtures/` do the
   rest. Drive the app through the helpers in `src/flows/wallet.ts` rather than repeating a
   receive or a balance read, reach the faucet through `src/faucet/FaucetClient.ts`, and
   devimint's other services through `src/devimint/`. Read the federation's generation with
   `currentShape()` from `src/shape.ts` wherever the two differ.
6. Assert outcomes, not text: the operation's final state, the structured error code
   (`readSdkError`), the exact balance change against the SDK's own quote (`waitForBalance`),
   and, where there is one, the other side of the payment. Every wait takes a timeout.
7. If a view the test needs has no id yet, add one in `activity_main.xml` — that is a smaller
   change than matching on text that the next copy edit breaks.
