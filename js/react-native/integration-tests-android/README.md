# @fedimint/integration-tests-android

Android device-level tests for the fedimint SDK, driven via [Appium](https://appium.io/)
against `js/examples/react-native/android`.

**This tests the SDK, not the example app.** `js/examples/react-native` is a minimal harness
for exercising `@fedimint/react-native` (which wraps `@fedimint/react-native-bindings`,
UniFFI-backed) on a real Android runtime — it has no product surface of its own. Tests here
are organized by SDK capability, mirroring the naming in
`js/web/integration-tests/src/services/*.test.ts` (the WASM/browser equivalent), not by
UI flow.

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
bash scripts/e2e-android/run-android-e2e.sh          # builds the APK, picks/boots a device, runs tests
```

Or drive the runner directly once Appium is running and a device is configured:

```bash
PLATFORM=android \
AVD=<avd-name> \
BUNDLE_PATH=/path/to/app-debug.apk \
APP_PACKAGE=com.reactnativeexample \
APP_ACTIVITY=com.reactnativeexample.MainActivity \
ts-node --project tsconfig.json src/runner.ts mnemonic
```

Pass `all` instead of a test name to run every registered test.

## Adding a new SDK-service test

1. Add `src/services/<Name>Service.test.ts` — a class extending `AppiumTestBase` with a
   single `execute()` method that throws to fail. No Jest matchers; see
   `MnemonicService.test.ts` for the shape.
2. Register it in `src/registry.ts`'s `availableTests` map.
3. If the test needs a starting state beyond a fresh install (e.g. an already-joined
   federation), add a fixture under `src/fixtures/` (see `src/fixtures/types.ts`) and declare
   `static prerequisites` on the test class — the runner resolves and caches fixtures across
   adjacent tests that share the same prerequisites.
4. If the test needs a real federation, use `src/faucet/FaucetClient.ts` to join/pay/invoice
   against the same devimint-backed faucet the WASM integration tests use (see
   `scripts/setup_test_shell.sh` for how that federation gets started).
5. Add `testID` props to `js/examples/react-native/src/App.tsx` only for elements with no
   stable, unique visible text (icon-only controls, value fields you read back, or — as with
   the mnemonic Generate button — text that collides with another element on screen). Prefer
   `clickOnText`/`isTextPresent` otherwise.
