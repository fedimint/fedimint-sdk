# @fedimint/react-native

The Fedimint SDK for React Native: `rust/fedimint-sdk` through its JSI turbo-module bindings
(`@fedimint/react-native-bindings`), the same API `@fedimint/web-sdk` exposes over a worker, but
called directly since there is no worker boundary to cross here.

## Implementation Options

Depending on your project setup, you can use the SDK with or without Expo. Choose the option
below that fits your environment.

### Option 1: Without Expo (Bare React Native)

#### Installation

```bash
npm install @fedimint/react-native@beta @fedimint/react-native-bindings@beta
# or
yarn add @fedimint/react-native@beta @fedimint/react-native-bindings@beta
# or
pnpm add @fedimint/react-native@beta @fedimint/react-native-bindings@beta
```

Install both. `@fedimint/react-native-bindings` is the native module, and React Native only links
native modules that the app's own `package.json` lists. The two packages are released together
and only work at the same version. The SDK is in beta and its releases are published under the
`beta` tag, which is why the commands name it.

The native library is built for the `arm64-v8a` and `x86_64` Android ABIs. Build the app for
those two only, in `android/gradle.properties`:

```properties
reactNativeArchitectures=arm64-v8a,x86_64
```

You'll also need `react-native-fs` for a documents directory to store data in:

```bash
npm install react-native-fs
```

#### Usage

```typescript
import { openSdk, Exception } from '@fedimint/react-native'
import RNFS from 'react-native-fs'

const dataDir = `${RNFS.DocumentDirectoryPath}/fedimint`
const { sdk, InviteCode, close } = await openSdk({ dataDir })

try {
  const federation = await sdk.join(InviteCode.parse(inviteCode))
  // ... use `federation` and the rest of the generated API.
} catch (error) {
  if (Exception.hasInner(error)) {
    const failure = Exception.getInner(error)
    console.error(failure.code(), failure.reason(), failure.details())
  }
  throw error
} finally {
  await close()
}
```

Pass what the user pastes or scans to the SDK as it is. `InviteCode.parse`, `Notes.parse` and the
string-typed values (a `Bolt11Invoice`, an `Address`) ignore whitespace around the text. A
`Bolt11Invoice`, such as the one `lightning.quote(invoice)` takes, also accepts a `lightning:`
scheme in any case, as a QR code often carries it. A `bitcoin:` (BIP-21) URI is not an invoice:
take the invoice or the address out of it first.

### Option 2: With Expo

#### Installation

```bash
npm install @fedimint/react-native@beta @fedimint/react-native-bindings@beta
npx expo install expo-file-system
```

For Expo managed workflow (SDK 52+), add the plugin to your `app.json`:

```json
{
  "expo": {
    "plugins": ["@fedimint/react-native"]
  }
}
```

Then build with EAS or a custom dev client:

```bash
npx expo prebuild
npx expo run:ios
# or
npx expo run:android
```

**Note:** Expo Go is not supported. You must use a custom dev client.

#### Usage

```typescript
import { openSdk } from '@fedimint/react-native'
import { Paths } from 'expo-file-system'

// Strip the file:// scheme from the documents directory URI to get a plain
// filesystem path for Rust.
const dataDir = Paths.document.uri.replace(/^file:\/\//, '') + 'fedimint'

const { sdk, InviteCode, close } = await openSdk({ dataDir })
const federation = await sdk.join(InviteCode.parse(inviteCode))
```

## Requirements

| React Native    | Support        |
| --------------- | -------------- |
| 0.78.x - 0.82.x | ✅ Supported   |
| 0.83.x          | ✅ Recommended |

| Platform | Minimum Version      | Architectures                          |
| -------- | -------------------- | -------------------------------------- |
| Android  | API 24 (Android 7.0) | `arm64-v8a`, `x86_64`                  |
| iOS      | 15.0                 | devices, simulators (arm64 and x86_64) |

| Expo SDK | Support          |
| -------- | ---------------- |
| 52+      | ✅ With plugins  |
| Expo Go  | ❌ Not supported |

## API

`openSdk({ dataDir, mnemonic? })` returns `{ sdk, InviteCode, Notes, Mnemonic, close }`. `sdk`
and everything reachable from it (federations, operations, quotes, and so on) is the generated
API of `@fedimint/react-native-bindings`: every class, enum and method it exports, called
directly since JSI calls need no round trip. This package re-exports that whole module, so any
type or function documented there (`InviteCode`, `Notes`, `Mnemonic`, tagged enums such as
`LnReceiveState`, and so on) is available from `@fedimint/react-native` directly.

A failure is thrown as an `Error` that carries an `Exception`, so `error instanceof Exception`
does not match it. `Exception.hasInner(error)` tells whether a thrown value is an SDK failure, and
`Exception.getInner(error)` takes the `Exception` out. On it, `code()` gives the stable error code
to branch on, `reason()` a human-readable message (never parsed, only logged), and `details()`
structured detail where the failure has any. The thrown error's `message` names the code and the
reason, so an uncaught failure reads as more than `Exception` in a log, and the `Exception`'s own
`toString()` reads `<code>: <reason>`.

`close()` shuts the SDK down and destroys it; every object obtained from the session is dead
afterward, and a later call waits on the same teardown and reports its outcome. Once destroyed,
`sdk` takes no calls at all, `exportMnemonic` included, which a bare `sdk.shutdown()` leaves
working: a call throws the bindings' error for a destroyed object rather than an `Exception`, so
export the seed before closing if you need it. Destroying it is what hands the data directory
back, so reopening that directory also means letting go of every other object still held from the
session, a federation, an operation, a subscription. Opening a directory something still holds
fails with `StorageInUse`.

## React hooks

`useBalance(federation, onError?)` and `useOperation(operation, onError?)` pull a generated
`…Updates` cursor for a component and return the latest value, or `undefined` before the first
one. They take the generated objects directly (`FederationLike`, and any operation such as
`LnReceiveOperationLike` or `RecoveryOperationLike`) and need no provider.

```tsx
import { Text } from 'react-native'
import {
  LnReceiveState_Tags,
  useBalance,
  useOperation,
  type FederationLike,
  type LnReceiveOperationLike,
} from '@fedimint/react-native'

function Balance({ federation }: { federation: FederationLike }) {
  const msat = useBalance(federation, (err) =>
    console.warn('balance updates stopped', err),
  )
  return <Text>{msat === undefined ? '…' : `${msat / 1000n} sats`}</Text>
}

function Invoice({ operation }: { operation: LnReceiveOperationLike }) {
  const state = useOperation(operation)
  return (
    <Text>
      {state?.tag === LnReceiveState_Tags.Claimed ? 'Paid' : 'Waiting'}
    </Text>
  )
}
```

- Each hook keeps one subscription per `id()`. A new handle for the same federation or operation
  does not restart it; a different id does, and the old value is dropped.
- Unmounting, or a new id, aborts the pending `next()` through its `signal`.
- An operation's final state stays after its stream ends.
- A failure to observe goes to `onError` once and is not retried, as the SDK reports a stream it
  cannot re-establish rather than spinning on it. `FederationClosed` means the handle is dead: after
  reopening, remount the component (for example with a React `key`) to watch the new one.

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
  type RecoveryOperationLike,
} from '@fedimint/react-native'

async function follow(progress: RecoveryOperationLike) {
  const updates = progress.updates()
  for (let state = await updates.next(); state; state = await updates.next()) {
    if (state.tag === RecoveryState_Tags.Running && state.inner.progress) {
      const { complete, total } = state.inner.progress
      console.log(`recovering: ${complete} of ${total}`)
    }
  }
}

const { progress } = await sdk.recover(invite)
void follow(progress)
```

A recovery carries on across restarts, and one that stopped is retried when the SDK opens again.
To watch it again after a restart, look for the federations that are still `Recovering` and ask
for their recovery by id. `resumeRecovery` hands back the attempt that is running, and starts a new
one if the last attempt stopped:

```ts
for (const federation of sdk.federations()) {
  const id = federation.id()
  const status = sdk.federationStatus(id)
  if (status?.tag !== FederationStatus_Tags.Recovering) continue
  const { progress } = await sdk.resumeRecovery(id)
  void follow(progress)
}
```

`sdk.recoveryStatus(id)` reads where a federation's recovery stands without starting anything, and
is `undefined` for a federation that was joined rather than recovered.

## Federation metadata

`federation.meta()` reads what a federation says about itself, such as its name or welcome
message. `get(key)` and `all()` give one merged view, with values as strings. For each key, the
first of three sources that defines it wins:

1. the consensus metadata of the federation's meta module, which the guardians can change;
2. the override document at the URL the federation's configuration names under
   `meta_override_url`. The client fetches it in the background, and the SDK reads that copy;
3. the configuration metadata, fixed for the life of the federation's configuration.

```ts
const meta = federation.meta()
const welcome = await meta.get('welcome_message')
```

`configMetadata()` and `consensusMetadata()` read those two sources on their own, for an app that
needs to know where a value came from, or the consensus revision. The override document has no
reader of its own: the client keeps it already merged with the configuration metadata.

## License

MIT
