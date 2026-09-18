# @fedimint/integration-tests-android

Android device-level tests for the fedimint SDK, driven via [Appium](https://appium.io/)
against the demo app in [`android/app`](../../../android).

**This tests the SDK, not the demo app.** `android/app` is one scrolling screen that calls
every export of `rust/fedimint-sdk`'s `uniffi` feature through the generated Kotlin bindings
— it has no product surface of its own. What these tests add over `kotlin-sdk.yaml`, which
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
nix develop .#android-tests
pnpm --dir js install   # first time only

bash scripts/e2e-android/setup-and-start-appium.sh   # one-time per shell: installs/starts Appium
bash scripts/e2e-android/run-android-e2e.sh          # builds the SDK + APK, picks/boots a device, runs tests
```

`run-android-e2e.sh` builds the whole Android payload first
(`scripts/build-android-sdk.sh`: the cross-compiled `.so` via Nix, then the Kotlin generated
from it) unless `SKIP_BINDINGS_BUILD=true` says both are already in place, which is what CI
passes after restoring them from the `native` job's artifact.

Or drive the runner directly once Appium is running and a device is configured:

```bash
PLATFORM=android \
AVD=<avd-name> \
BUNDLE_PATH=android/app/build/outputs/apk/debug/app-debug.apk \
APP_PACKAGE=org.fedimint.demo \
APP_ACTIVITY=org.fedimint.demo.MainActivity \
ts-node --project tsconfig.json src/runner.ts mnemonic
```

Pass `all` instead of a test name to run every registered test.

## Naming a view

`clickElementByKey`/`getTextByKey`/`typeIntoElementByKey` take the bare id a view carries in
[`activity_main.xml`](../../../android/app/src/main/res/layout/activity_main.xml) — `openWallet`,
`walletResult`, `seed`. `AppiumTestBase` qualifies it with `APP_PACKAGE` into the
`org.fedimint.demo:id/openWallet` resource-id that UiAutomator2 matches on, so a test never
repeats the package. Prefer these over `clickOnText`/`isTextPresent`: an id is stable across
copy changes, and several sections of the demo share button labels.

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
   before any test whose `static prerequisites` don't include what is on the device.
4. If the test needs a starting state beyond a fresh install (e.g. an already-joined
   federation), add a fixture under `src/fixtures/` (see `src/fixtures/types.ts`) and declare
   `static prerequisites` on the test class — the runner resolves and caches fixtures across
   adjacent tests that share the same prerequisites.
5. If the test needs a real federation, use `src/faucet/FaucetClient.ts` to join/pay/invoice
   against the same devimint-backed faucet the WASM integration tests use (see
   `scripts/setup_test_shell.sh` for how that federation gets started). The demo's Join
   section takes an invite code, so a fixture can paste one in rather than the test hardcoding
   a federation.
6. If a view the test needs has no id yet, add one in `activity_main.xml` — that is a smaller
   change than matching on text that the next copy edit breaks.
