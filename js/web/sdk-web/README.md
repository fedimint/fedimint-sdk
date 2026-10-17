# @fedimint/sdk-web

The Fedimint SDK for browsers: `rust/fedimint-sdk` compiled to WebAssembly and hosted in a
dedicated worker, so every call from the main thread is asynchronous, including the ones that
are synchronous in Rust.

## Usage

```ts
import { openSdk, LnReceiveState_Tags } from '@fedimint/sdk-web'

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
} from '@fedimint/sdk-web'

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

The API is the generated one under
[`src/generated/fedimint_sdk.ts`](./src/generated/fedimint_sdk.ts): every class, enum and
method it exports, with every method returning a promise on the main thread.

## Regenerating the bindings

The TypeScript in `src/generated/` is committed, but it is generated, not written by hand.
Regenerate it from a source checkout with:

```sh
pnpm generate
```

run inside the `.#wasm` Nix shell (`nix develop .#wasm`). This builds
`rust/fedimint-sdk` for `wasm32-unknown-unknown` through Nix and turns the result into the
files under `src/generated/`; see `scripts/generate-sdk-web-bindings.sh` for the details.

## Testing

The browser test that exercises this package against a running federation is run with:

```sh
just test
```

from the repository root.

## Size of the wasm module

A browser downloads the module this package ships, `src/generated/fedimint_sdk.wasm`, before the
SDK can do anything. On a pull request, the `Verify / Check Wasm Size` check compares the size of
that module, and its size after `gzip -9`, with those of the commit of the base branch it is
merged into. It fails when the pull request makes either size grow by more than 5%, and its
summary has a table of the sizes and their differences. When the base commit's module cannot be
fetched or generated from, the check shows the sizes of the pull request alone and passes with a
warning. It does not run when the `Verify / Test` job fails before it has built the module.

To get the numbers for a module of your own, generate the bindings and measure the module from the
repository root:

```sh
just generate-sdk-web-bindings
scripts/sdk-web-wasm-size.sh measure js/web/sdk-web/src/generated/fedimint_sdk.wasm
```

It prints the size of the module and its size after `gzip -9`, both in bytes. The second moves by a
hundred bytes or so between two runs of the generator, even on the same crate.
