---
title: 'meta()'
description: 'Gives the metadata of the federation.'
---

# FederationLike.meta

<!-- signature: FederationLike['meta'] -->

```ts
meta(): MetaLike
```

Gives the metadata of the `federation`. Synchronous.

**Parameters.** None.

**Returns.** `MetaLike`. The metadata object of the `federation`.

**Throws.** None.

**Example**
{#example}

```ts [example.ts]
import type { FederationLike } from '@fedimint/react-native'

declare const fed: FederationLike

const meta = fed.meta() // [!code focus]
console.log(meta.configMetadata().get('welcome_message'))
```

**Used in.** Not used in a guide step.
