---
title: 'Migration: explicit wallet setup'
description: 'Opening storage no longer creates a wallet seed automatically.'
---

# Migration: explicit wallet setup

::: warning Unreleased API
These instructions require matching source-built `@fedimint/react-native` and
`@fedimint/react-native-bindings` from this change. Both latest published packages are
`0.1.0`, which does not include `hasMnemonic` or `setMnemonic`. No released package pair
ships this API yet; the first shipping pair must be documented when it is released.
Use the [pinned installation guide](./installation.md) for released packages.
:::

`openSdk` still opens one shared session. Existing wallets load their stored seed. Empty
storage opens without a seed, so discovery, invite parsing and preview can run before setup.
Opening may create a database or directory; it does not generate a mnemonic.

Check `sdk.hasMnemonic()` to choose between wallet and setup screens. On the same session,
initialize only after the user chooses Create or submits Restore words:

```ts [setup.ts]
import { openSdk } from '@fedimint/react-native'

declare const dataDir: string
declare const choice: { kind: 'create' } | { kind: 'restore'; words: string[] }

const session = await openSdk({ dataDir })
if (choice.kind === 'restore') {
  // Always validate explicit Restore words, even if storage already has a seed.
  // SeedMismatch stops this flow; never continue with a different stored wallet.
  await session.sdk.setMnemonic(session.Mnemonic.fromWords(choice.words))
} else if (!session.sdk.hasMnemonic()) {
  await session.sdk.setMnemonic(session.Mnemonic.generate())
}
```

For Create, show `sdk.exportMnemonic().words()` privately and confirm the backup before joining.
For Restore, recover the user's federations. Until setup, export, join and recovery fail with
`MnemonicRequired`; **export is now fallible**. On web, also await `hasMnemonic`, mnemonic
creation/parsing, export and `words`, because those calls go through the worker.

`setMnemonic` persists the seed before reporting success. Repeating the same seed is harmless;
different words, corrupt storage or wallet state without a usable seed are refused. Never
delete or replace existing wallet storage to bypass a refusal. No second opening mode or
`WalletNotFound` flag is needed.
