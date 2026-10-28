---
title: 'close()'
description: 'Shuts down the SDK and destroys it.'
---

# SdkSession.close

<!-- signature: SdkSession['close'] -->

```ts
close(): Promise<void>
```

Shuts down the SDK and destroys it.

**Parameters.** None.

**Returns.** `Promise<void>`. Resolves after `sdk.shutdown()` and `sdk.uniffiDestroy()` finish. A later call shares the same teardown.

**Throws.** [`Storage`](./errors.md#code-storage). Storage means the final flush failed. The SDK is still destroyed. After close, a call on sdk throws a bindings error that is not an Exception. A data directory that stays held gives StorageInUse on the next open.

**Example**
{#example}

```ts [example.ts]
import type { SdkSession } from '@fedimint/react-native'

declare const session: SdkSession

await session.close() // [!code focus]
```

**Used in.** [Seed words, step 01](./seed-words.md#step-01), [Session, step 01](./session.md#step-01), [Installation, step 06](./installation.md#step-06)
