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

`js/react-native/integration-tests-android` tests the SDK on a real Android runtime, driven via
[Appium](https://appium.io/) against `js/examples/react-native/android`. This tests the SDK, not
the example app — `js/examples/react-native` is a minimal harness for exercising
`@fedimint/react-native` (which wraps `@fedimint/react-native-bindings`, UniFFI-backed), not a
product with its own UI surface. Tests are organized by SDK capability (mirroring
`js/web/integration-tests/src/services/*.test.ts`'s naming), not by UI flow.

**Why Appium and not Detox/Maestro:** Detox is React-Native-only and synchronizes tightly with
RN's own bridge/event loop, so it can't be reused if this SDK is ever driven from outside RN.
Appium/UiAutomator2 drives the Android accessibility tree the same way regardless of what
produced the view, and covers iOS under the same tool if that's ever revisited. Maestro is a
legitimate lighter alternative (less boilerplate, YAML flows) but has less programmatic
flexibility for the state/fixture logic this harness uses, and a smaller ecosystem. iOS is out
of scope for now.

**Nix**: run this from the `android-tests` devshell (`nix develop .#android-tests`), which extends
the plain `android` FFI-build shell with an emulator + system image and wires
`PATH`/`APPIUM_HOME` around the pnpm-installed `appium` binary — Appium itself is a plain npm
devDependency, not a Nix package; Nix only supplies the Android SDK/emulator toolchain around
it. See `js/react-native/integration-tests-android/README.md` for how to run it and
add a new SDK-service test.
