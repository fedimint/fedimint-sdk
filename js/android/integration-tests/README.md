# @fedimint/integration-tests-android

Android device-level tests for the fedimint SDK, driven via [Appium](https://appium.io/)
against the example app in [`android/app`](../../../android).

**This tests the SDK, not the example app.** `android/app` is one scrolling screen that calls
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
just test-android-e2e
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

CI makes the same split across machines, as separate jobs in `kotlin-sdk.yaml`. Each task runs
exactly once and the next job downloads its output rather than redoing it:

| job        | workflow                            | shell             | produces                                                  |
| ---------- | ----------------------------------- | ----------------- | --------------------------------------------------------- |
| `native`   | `android-native.yaml` (self-hosted) | —                 | `jniLibs`, the cross-compiled `.so`                       |
| `bindings` | `kotlin-sdk.yaml`                   | — (Rust)          | `kotlin-bindings`, generated once from that `.so`         |
| `apk`      | `android-apk.yaml`                  | `.#android`       | `android-example-apk` — Gradle on the two artifacts above |
| `e2e`      | `android-e2e.yml`                   | `.#android-tests` | the run itself — installs the APK, no Gradle              |

The AAR job (`kotlin`) downloads the same `jniLibs` and `kotlin-bindings` and only assembles the
release AAR; the example app is compiled once, by `apk`.

By hand, the two halves:

```bash
just build-android-apk                       # `.#android`: native lib, Kotlin, Gradle

nix develop .#android-tests
pnpm --dir js install                        # first time only
bash scripts/e2e-android/setup-and-start-appium.sh   # one-time per shell: installs/starts Appium
bash scripts/setup_test_shell.sh bash scripts/e2e-android/run-android-e2e.sh
```

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
and the app joins through the unmodified code like any other client. A test
that needs a service outside that window (esplora, say, for an on-chain
deposit) has to add its port to `reverse_devimint_ports`.

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
   receive or a balance read, and reach the faucet through `src/faucet/FaucetClient.ts`.
6. If a view the test needs has no id yet, add one in `activity_main.xml` — that is a smaller
   change than matching on text that the next copy edit breaks.
