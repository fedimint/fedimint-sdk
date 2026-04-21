# Web SDK Development

This guide explains how to set up, develop, and build the Web SDK for Fedimint.

The Web SDK packages in this repository bridge the Rust-based `fedimint-client-wasm` crate into browser and Node.js environments.

## Dependency on Fedimint Client WASM

**Important:**

- The WASM binary is required for the Web SDK packages to function.
- It is built using the source code from the `fedimint` repository, not this SDK repository.
- The specific revision of the core `fedimint` repository used is defined as a flake input in `flake.nix`.

### Updating the WASM Binary

If you have made changes to the Rust side (in the `fedimint` repo) and need to update the WASM binary used by this SDK:

1. Update the `fedimint-wasm` input in [flake.nix](https://github.com/fedimint/fedimint-sdk/blob/main/flake.nix) (around line 9) to point to the new commit/revision.
2. Build the WASM binary (see instructions below).

## Building the WASM Binary

First, ensure you have entered the Nix development shell:

```bash
nix develop
```

Then, you must build the WASM binary:

```bash
pnpm build:wasm
pnpm build
```

This command will build the `fedimint-client-wasm` crate and place the resulting binary in the `packages/wasm-bundler` folder. The subsequent command (`pnpm build`) is required to package the TypeScript wrapper libraries for consumption.

## Running the dev playgrounds

To start the local development playgrounds, run one of the following commands. These commands run playground apps, located at `./examples`, that are set up for trying out code while making changes.

**Important Note for Local Examples:**
To ensure the examples load your local development changes instead of fetching from the published NPM registry, you must update the example's `package.json` to use local workspace paths. Change the Fedimint dependencies to `workspace:*`:

```json
{
  "dependencies": {
    "@fedimint/core": "workspace:*",
    "@fedimint/transport-web": "workspace:*"
  }
}
```

After modifying the example's `package.json`, run `pnpm install` from the root of the repository to link the local workspaces.

Then, you can start the playground dev servers:

```bash
pnpm dev              # aliased to `pnpm dev:core`
pnpm dev:core         # `@fedimint/core` + Vite + React app
pnpm dev:next         # `@fedimint/core` + Next.js app
# pnpm dev:react      # TBD
pnpm dev:bare         # HTML + VanillaJS app (no framework)
```

Once a playground dev server is running, you can make changes to any of the package source files (e.g. `packages/react`) and it will automatically update the playground.

## Running the test suite

The Fedimint Web SDK uses [Vitest](https://vitest.dev) to run tests.
_Note: The React Native SDK is currently not covered by these tests._

See the [testing docs](./testing.md) for more information.
