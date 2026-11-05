# @fedimint/web-sdk

The Fedimint SDK for browsers: [`rust/fedimint-sdk`](https://github.com/fedimint/fedimint-sdk/tree/main/rust/fedimint-sdk)
compiled to WebAssembly and hosted in a dedicated worker, so every call from the main thread is
asynchronous, including the ones that are synchronous in Rust. The API is generated from the
Rust one, so it is a view of that API and not a copy of it.

## Install

```sh
npm install @fedimint/web-sdk@beta
```

## Usage

```ts
import { openSdk, LnReceiveState_Tags } from '@fedimint/web-sdk'

const session = await openSdk({ storage: 'my-wallet' })
const invite = await session.InviteCode.parse(code)
const federation = await session.sdk.join(invite)
const { invoice, operation } = await (await federation.lightning())!.receive(
  100_000n,
  'coffee',
)
```

Three things follow from the SDK running in a dedicated worker rather than on the calling
thread:

- Every method on every object returns a promise, including the ones that are synchronous in
  the Rust API: each call is a round trip to the worker.
- Objects (`Sdk`, `Federation`, operations, quotes, and so on) are handles rather than the real
  thing: the main thread holds a proxy keyed to an object that lives in the worker. Those
  handles die with the session, so call `session.close()` when done with it.
- Tagged enums (`FederationStatus`, `LnReceiveState`, and so on) cross as a plain object,
  `{ tag, ...fields }`, not a class instance. Switch on the matching `*_Tags` enum, exported
  alongside the SDK's own types (`LnReceiveState_Tags` above), rather than on a string literal.

## What the browser has to provide

- **A module worker.** The SDK runs in one, and every current browser has them.
- **A secure context.** The wallet's data lives in the origin's private file system (OPFS),
  which the worker opens through sync access handles. Browsers only hand those out on `https:`
  and on `localhost`, so a page served over plain `http:` from another host cannot open the
  SDK.
- **The module.** The browser downloads `fedimint_sdk.wasm`, about 12 MB (4.5 MB after gzip),
  before the SDK can do anything. Serve it compressed, and let the browser cache it.

No cross-origin isolation headers (COOP, COEP) are needed: the SDK does not use shared memory.

## Bundlers

The package finds its worker and its module with `new URL('…', import.meta.url)`, which Vite,
webpack 5 and the other current bundlers rewrite when they copy the files. No plugin is
needed for either.

With Vite, keep the package out of dependency pre-bundling, which does not follow those URLs
in dev mode:

```ts
// vite.config.ts
export default defineConfig({
  optimizeDeps: { exclude: ['@fedimint/web-sdk'] },
})
```

A host that has to load the worker from a URL of its own passes `createWorker` to `openSdk`;
the worker's entry point is exported as `@fedimint/web-sdk/worker`.

## Recovering a wallet

`sdk.recover(invite)` joins a federation and rescans its history for what the seed owns. It
returns `{ federation, progress }` as soon as the rescan starts, and `progress` is the operation to
watch. Until it reads `Done`, sends and receives on that federation fail with `Recovering`. While
it reads `Running`, `inner.progress` counts the rescan's work, `complete` out of `total`, once the
rescan has said how much work it has. At `Done` the balance holds all the ecash the rescan found.

```ts
import {
  FederationStatus_Tags,
  RecoveryState_Tags,
  type Proxied,
  type RecoveryOperationLike,
} from '@fedimint/web-sdk'

async function follow(progress: Proxied<RecoveryOperationLike>) {
  const updates = await progress.updates()
  for (let state = await updates.next(); state; state = await updates.next()) {
    if (state.tag === RecoveryState_Tags.Running && state.inner.progress) {
      const { complete, total } = state.inner.progress
      console.log(`recovering: ${complete} of ${total}`)
    }
  }
}

const { progress } = await session.sdk.recover(invite)
void follow(progress)
```

A recovery carries on across restarts, and one that stopped is retried when the SDK opens again.
To watch it again after a restart, look for the federations that are still `Recovering` and ask
for their recovery by id. `resumeRecovery` hands back the attempt that is running, and starts a new
one if the last attempt stopped:

```ts
for (const federation of await session.sdk.federations()) {
  const id = await federation.id()
  const status = await session.sdk.federationStatus(id)
  if (status?.tag !== FederationStatus_Tags.Recovering) continue
  const { progress } = await session.sdk.resumeRecovery(id)
  void follow(progress)
}
```

`sdk.recoveryStatus(id)` reads where a federation's recovery stands without starting anything, and
is `undefined` for a federation that was joined rather than recovered.

## API

The API is the generated one in
[`src/generated/fedimint_sdk.ts`](https://github.com/fedimint/fedimint-sdk/blob/main/js/web/web-sdk/src/generated/fedimint_sdk.ts):
every class, enum and method it exports, with every method returning a promise on the main
thread. The package ships it as `dist/generated/fedimint_sdk.d.ts`.

## Example

[`js/examples/vite-app`](https://github.com/fedimint/fedimint-sdk/tree/main/js/examples/vite-app)
is a React application built with Vite that opens a wallet, joins a federation, and sends and
receives ecash, Lightning and on-chain payments with this package.

## License

MIT
