---
title: 'exportMnemonic()'
description: 'Gives the seed phrase object of the wallet.'
---

# SdkLike.exportMnemonic

<!-- signature: SdkLike['exportMnemonic'] -->

```ts
exportMnemonic(): MnemonicLike
```

Gives the seed phrase object of the wallet. Synchronous.

**Parameters.** None.

**Returns.** `MnemonicLike`. The seed phrase object. Call `words()` on it to read the `words`.

**Throws.** None. The method is infallible. After SdkSession.close() the object is destroyed, so export first.

**Example**
{#example}

```ts [example.ts]
import type { SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike

const mnemonic = sdk.exportMnemonic() // [!code focus]
console.log(mnemonic.words().length)
```

**Used in.** [Seed words, step 02](./seed-words.md#step-02), [Restore, step 02](./restore.md#step-02), [Installation, step 06](./installation.md#step-06)
