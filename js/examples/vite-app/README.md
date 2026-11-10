# Vite Example

A React application built with [Vite](https://vite.dev) that uses `@fedimint/web-sdk`: it opens
a wallet, joins a federation, and sends and receives ecash, Lightning and on-chain payments.
Each section of the page is one part of the SDK.

## Technologies used

- `@fedimint/web-sdk`: the Fedimint SDK for browsers.
- Vite: the build tool and development server. [`vite.config.ts`](./vite.config.ts) has the one
  setting the SDK needs in dev mode.
- React.

## Running the application locally

From the repository root, build the wasm module and the package, then install the workspace:

```sh
just generate-sdk-web-bindings
pnpm --dir js install
```

Then start the dev server, or build and serve the production bundle:

```sh
cd js/examples/vite-app
pnpm dev                      # http://localhost:5173
pnpm build && pnpm preview    # http://localhost:4173
```

## Against a local federation

The page can take an invite code from devimint's faucet, and pay the invoices it creates through
it, so one wallet is enough to try everything. Start a federation from the repository root with:

```sh
nix develop --accept-flake-config .#wasm-tests -c scripts/setup_test_shell.sh bash
```

and point the page at the faucet port it exports, in the field under "Join a federation" or with
`?faucet=http://localhost:$FM_PORT_FAUCET`. Any other federation works too: paste its invite code
instead of fetching one.
