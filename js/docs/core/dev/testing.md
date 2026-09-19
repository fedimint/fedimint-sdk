# Testing

We use [vitest](https://vitest.dev/) for testing library code.

Configuring this properly was tricky. Since the library heavily relies on browser APIs like web workers & wasm, it doesn't really make sense to mock the browser APIs for unit tests.
In order for our tests to be trustworthy, we really need them to run in a realistic browser environment.

Vitest [browser mode](https://vitest.dev/guide/browser/) + playwright (provider) seems to satisfy all our needs. It spins up a real browser to run tests and can run headlessly for CI.

This framework should be suitable for all the additional libraries we have planned (e.g. react).

## Nix

The Fedimint Sdk depends on several external pieces of infrastructure. In order to run high-fidelity tests, we utilize a tool from the [fedimint](https://github.com/fedimint/fedimint) repo called [Devimint](https://github.com/fedimint/fedimint/tree/master/devimint). Devimint includes several pieces of infrastructure for running a local testing environment for fedimint applications including a bitcoind node (regtest), multiple guardian servers (fedimintd), multiple lightning gateways (lnd, cln, ldk), and a faucet for minting tokens.

::: warning Note

Nix is NOT required to build or use the Fedimint Sdk. It is ONLY required to run the tests.

:::

## Nix Installation & Setup

To setup nix, use the [Determinate Nix Installer](https://github.com/DeterminateSystems/nix-installer)

```sh
# The exact version might be different.
> nix --version
nix (Nix) 2.9.1
```

Next, [install direnv](https://direnv.net/docs/installation.html) and run the following command to initialize direnv in your shell:

```sh
direnv allow
```

::: tip
This takes a really long time to run for the first time. All future runs will be relatively quick.
:::

## Usage

```bash
# in the js/ workspace root
pnpm run test
```

- `pnpm test` — runs tests in a headless browser
- `pnpm test:cov` — runs tests and reports coverage
- `pnpm test:ui` — runs tests in the [Vitest UI](https://vitest.dev/guide/ui.html)

When adding new features or fixing bugs, it's important to add test cases to cover the new or updated behavior.

## Android E2E (Appium)

`js/android/integration-tests` tests the SDK on a real Android runtime, driven via
[Appium](https://appium.io/) against the demo app in `android/app`. This tests the SDK, not
the demo — `android/app` is one screen that calls every export of `rust/fedimint-sdk`'s
`uniffi` feature through the generated Kotlin bindings, not a product with its own UI
surface. What it adds over `kotlin-sdk.yaml`, which compiles the same app, is a running
device: the bindings load, the native library is mapped, and the calls execute. Tests are
organized by SDK capability (mirroring `js/web/integration-tests/src/services/*.test.ts`'s
naming), not by UI flow.

**Why Appium and not Espresso/Maestro:** Espresso runs inside the app process and is Android
only, so a test written against it can never be reused for another platform this SDK is
driven from. Appium/UiAutomator2 drives the Android accessibility tree from outside the app,
the same way regardless of what produced the view, and covers iOS under the same tool if that
is ever revisited. Maestro is a legitimate lighter alternative (less boilerplate, YAML flows)
but has less programmatic flexibility for the state/fixture logic this harness uses, and a
smaller ecosystem. iOS is out of scope for now.

Like the WASM suite, it runs against a **local devimint federation**: the run is exec'd
inside `devimint wasm-test-setup` by the same `scripts/setup_test_shell.sh`, tests ask the
faucet for an invite code and for invoice payments, and the app joins the real thing. The
emulator reaches guardians and gateways bound to the host's `127.0.0.1` through `adb
reverse` — see `js/android/integration-tests/README.md` for why that rather than
`10.0.2.2`. `just test-android-e2e` is the Android counterpart of `just test`;
`just test-android-e2e-local` skips the federation for the tests that never join one.

**Nix**: run this from the `android-tests` devshell (`nix develop .#android-tests`), which extends
the plain `android` FFI-build shell with an emulator + system image and wires
`PATH`/`APPIUM_HOME` around the pnpm-installed `appium` binary — Appium itself is a plain npm
devDependency, not a Nix package; Nix only supplies the Android SDK/emulator toolchain around
it. See `js/android/integration-tests/README.md` for how to run it and
add a new SDK-service test.
