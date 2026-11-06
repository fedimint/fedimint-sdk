---
title: 'useBalance()'
description: 'A React hook that follows the balance of a federation.'
---

# useBalance

<!-- signature: typeof useBalance -->

```ts
useBalance(federation: FederationLike | undefined, onError?: (err: unknown) => void): Amount | undefined
```

A React hook that follows the balance of a `federation`.

**Parameters**

| Name         | Type                          | Description                                   |
| ------------ | ----------------------------- | --------------------------------------------- |
| `federation` | `FederationLike \| undefined` | The `federation` to watch.                    |
| `onError`    | `(err: unknown) => void`      | Optional. Receives the failure of the stream. |

**Returns.** `Amount | undefined`. The live balance in msat (a bigint). It is `undefined` until the first value arrives, and after a failure.

**Throws.** The hook does not throw. It reports a failure to onError, with FederationClosed once the federation or the SDK is closed. It does not retry.

**Example**
{#example}

```tsx [example.tsx]
import { useBalance, type FederationLike } from '@fedimint/react-native'

export function useBalanceSats(fed: FederationLike | undefined) {
  const msat = useBalance(fed, (err) => console.log('balance watch', err)) // [!code focus]
  return msat === undefined ? undefined : msat / 1000n
}
```

**Used in.** [Quickstart, step 02](./quickstart.md#step-02), [Balance, step 02](./balance.md#step-02), [Session, step 03](./session.md#step-03)
