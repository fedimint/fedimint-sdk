---
title: 'federations()'
description: 'Lists the open federations.'
---

# SdkLike.federations

<!-- signature: SdkLike['federations'] -->

```ts
federations(): Array<FederationLike>
```

Lists the open federations. Synchronous.

**Parameters.** None.

**Returns.** `Array<FederationLike>`. Every open `federation`. The order is not specified.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike

for (const fed of sdk.federations()) {
  // [!code focus]
  console.log(fed.id())
}
```

**Used in.** [Restore, step 04](./restore.md#step-04), [Join, step 03](./join.md#step-03), [Session, step 02](./session.md#step-02)
