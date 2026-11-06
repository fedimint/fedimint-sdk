---
title: 'receive()'
description: 'Redeems ecash notes.'
---

# EcashLike.receive

<!-- signature: EcashLike['receive'] -->

```ts
receive(notes: NotesLike): Promise<EcashReceiveOperationLike>
```

Redeems ecash notes.

**Parameters**

| Name    | Type        | Description                                       |
| ------- | ----------- | ------------------------------------------------- |
| `notes` | `NotesLike` | The notes to redeem. Get them from `Notes.parse`. |

**Returns.** `EcashReceiveOperationLike`. The redeem operation. Follow it with `updates()` or `awaitFinal()`.

**Throws.** [`InvalidInput`](./errors.md#code-invalidinput), [`Recovering`](./errors.md#code-recovering), [`NotSupported`](./errors.md#code-notsupported), [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`Storage`](./errors.md#code-storage), [`FederationClosed`](./errors.md#code-federationclosed). InvalidInput means malformed notes, or notes from another federation.

**Example**
{#example}

```ts [example.ts]
import { Notes, type EcashLike } from '@fedimint/react-native'

declare const ecash: EcashLike
declare const pasted: string

const operation = await ecash.receive(Notes.parse(pasted.trim())) // [!code focus]
console.log((await operation.awaitFinal()).tag)
```

**Used in.** [Receive, step 02](./receive.md#step-02)
