# Testing

We use [vitest](https://vitest.dev/) for testing library code.

Configuring this properly was tricky. Since the library heavily relies on browser APIs like web workers & wasm, it doesn't really make sense to mock the browser APIs for unit tests.
In order for our tests to be trustworthy, we really need them to run in a realistic browser environment.

Vitest [browser mode](https://vitest.dev/guide/browser/) + playwright (provider) seems to satisfy all our needs. It spins up a real browser to run tests and can run headlessly for CI. I had to add one hack to make it work with the web-worker, but otherwise it seems to work well out of the box.

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
# in the root of the repo
pnpm run test
```

- `pnpm test` — runs tests in a headless browser
- `pnpm test:cov` — runs tests and reports coverage
- `pnpm test:ui` — runs tests in the [Vitest UI](https://vitest.dev/guide/ui.html)

When adding new features or fixing bugs, it's important to add test cases to cover the new or updated behavior.
