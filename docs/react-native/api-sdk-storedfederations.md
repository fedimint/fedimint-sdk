---
title: 'storedFederations()'
description: 'Lists every federation in storage, running or not.'
---

# SdkLike.storedFederations

<!-- signature: SdkLike['storedFederations'] -->

```ts
storedFederations(): Array<FederationInfo>
```

Lists every `federation` in storage, running or not. Synchronous.

**Parameters.** None.

**Returns.** `Array<FederationInfo>`. One record for each stored `federation`, with its status.

**Throws.** None. The method is infallible. It still answers after shutdown.

**Example**
{#example}

```ts [example.ts]
import type { SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike

const [stored] = sdk.storedFederations() // [!code focus]
console.log(stored?.id)
```

**Used in.** [Quickstart, step 02](./quickstart.md#step-02), [Join, step 03](./join.md#step-03)
