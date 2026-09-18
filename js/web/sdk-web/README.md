# @fedimint/sdk-web

The Fedimint SDK for browsers: `rust/fedimint-sdk` compiled to WebAssembly and hosted in a
dedicated worker, so every call from the main thread is asynchronous, including the ones that
are synchronous in Rust.

## API

The API is the generated one under
[`src/generated/fedimint_sdk.ts`](./src/generated/fedimint_sdk.ts): every class, enum and
method it exports, with every method returning a promise on the main thread.

## Regenerating the bindings

The TypeScript in `src/generated/` is committed, but it is generated, not written by hand.
Regenerate it from a source checkout with:

```sh
pnpm generate
```

run inside the `.#wasm-tests` Nix shell (`nix develop .#wasm-tests`). This builds
`rust/fedimint-sdk` for `wasm32-unknown-unknown` through Nix and turns the result into the
files under `src/generated/`; see `scripts/generate-sdk-web-bindings.sh` for the details.

## Testing

The browser test that exercises this package against a running federation is run with:

```sh
just test
```

from the repository root.
