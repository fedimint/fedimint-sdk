---
title: 'Join a federation'
description: 'Read an invite code, preview a federation, join it, and reconnect on every launch.'
---

# Join a federation

Read an invite code, show the user what they are joining, then join and reconnect on every launch.
{.subtitle}

<p class="cap-row"><Badge type="tip" text="parsing a code needs no network" /> <Badge type="tip" text="preview writes nothing to storage" /> <Badge type="danger" text="leaving needs a zero balance" /></p>

A federation is a group of guardians who between them issue the ecash your wallet will hold. Joining one asks the guardians for their configuration and writes it to the wallet's folder. After this page your wallet has somewhere to keep money.

Leaving is hard. `sdk.forgetFederation(id)` erases a federation only when it holds no money and has no pending operation. So a screen that joins should show the user what they are joining first. That is why the preview comes before the join.

::: info
**Practise on money that is not real.** Fedi Testnet runs on signet, where coins have no value. Pay it from a signet faucet. Its invite code is: `fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75`
:::

### 01 · Reject a bad code without a network call <Badge type="info" text="JavaScript" /> {#step-01}

An invite code is a string that starts with `fed1`. `InviteCode.parse` reads it on the device, with no network call. Call it as soon as the user pastes or scans, and tell them at once if the code is bad.

```ts [read-invite.ts]
import {
  ErrorCode,
  Exception,
  InviteCode,
  type InviteCodeLike,
} from '@fedimint/react-native'

/** The parsed invite, or null when the pasted text is not a code. No network. */
export function readInvite(pasted: string): InviteCodeLike | null {
  try {
    // parse does not trim. Remove the spaces and line breaks a paste brings with it.
    return InviteCode.parse(pasted.trim())
  } catch (err) {
    if (
      Exception.hasInner(err) &&
      Exception.getInner(err).code() === ErrorCode.InvalidInput
    ) {
      return null
    }
    throw err
  }
}
```

A code that does not parse throws an error with the code `ErrorCode.InvalidInput`. The error is not an `instanceof` any class, so read its code with `Exception.hasInner` and `Exception.getInner`. [`invite.federationId()`](./api-values-invitecode-federationid.md) gives the federation's id, and `invite.display()` gives the code back as a string.

::: tip Check
`readInvite('hello')` returns `null`. `readInvite` with the Fedi Testnet code returns an invite, and its `federationId()` is a 64 character hex string.
:::

### 02 · Show the user what they are about to join <Badge type="info" text="JavaScript" /> {#step-02}

`sdk.preview(invite)` asks the guardians for the federation's configuration. It writes nothing to storage, so it is safe to call on a code the user has not agreed to yet. It also checks that this SDK can use the federation. A federation it cannot use fails here with `UnsupportedFederation`, not later at the join.

```ts [preview-federation.ts]
import {
  Network,
  type InviteCodeLike,
  type SdkLike,
} from '@fedimint/react-native'

export async function previewForAScreen(sdk: SdkLike, invite: InviteCodeLike) {
  // Asks the guardians for their configuration. Writes nothing.
  const preview = await sdk.preview(invite)

  return {
    // undefined when the federation published no name. Do not invent one.
    name: preview.name ?? `federation ${preview.id.slice(0, 8)}`,
    // Other fields the federation published, as a Map of strings.
    welcome: preview.meta.get('welcome_message'),
    guardians: preview.guardians,
    modules: preview.modules,
    // Network.Bitcoin is real money. Signet, Testnet, Testnet4 and Regtest are not.
    realMoney: preview.network === Network.Bitcoin,
  }
}
```

`preview.guardians` is the number of guardians. `preview.modules` lists the module names the federation runs, such as `mint`, `ln` and `wallet`.

::: warning
**Show the network.** `preview.network` says which chain this federation's money is on. `Network.Bitcoin` is real money. A test build should refuse it, and a real build should say so plainly before the user joins.
:::

::: tip Check
With the Fedi Testnet code, you will see a name, a guardian count above zero, and `realMoney` set to `false`.
:::

### 03 · Join, then reconnect on every launch <Badge type="info" text="JavaScript" /> {#step-03}

`sdk.join(invite)` returns the federation, open and ready. The invite is only needed for this first join. The wallet keeps the federation in its own folder, so your app does not need to save the invite.

```ts [join-federation.ts]
import type {
  FederationLike,
  InviteCodeLike,
  SdkLike,
} from '@fedimint/react-native'

/** After the user said yes on the preview screen. */
export async function joinFederation(
  sdk: SdkLike,
  invite: InviteCodeLike,
): Promise<FederationLike> {
  return sdk.join(invite)
}
```

Joining a federation the wallet already holds fails with `AlreadyJoined`. On every later launch, use the function below. It lists the federations in the wallet's folder with [`sdk.storedFederations()`](./api-sdk-storedfederations.md). It needs no saved invite. It returns `undefined` when the wallet holds no federation yet. Then show your Join screen.

```ts [federation-on-launch.ts]
import type { FederationLike, SdkLike } from '@fedimint/react-native'

/** On every launch. Returns undefined when the wallet has not joined a federation. */
export async function federationOnLaunch(
  sdk: SdkLike,
): Promise<FederationLike | undefined> {
  const [stored] = sdk.storedFederations()
  if (!stored) return undefined
  return sdk.federation(stored.id) ?? (await sdk.reopenFederation(stored.id))
}
```

| The federation is                       | `federationOnLaunch` does                                    |
| --------------------------------------- | ------------------------------------------------------------ |
| already open in this session            | `sdk.federation(id)` hands it back                           |
| joined or restored before, not open yet | `sdk.reopenFederation(id)` opens it from the wallet's folder |
| never joined                            | returns `undefined`                                          |

`storedFederations()` lists every federation in the folder. A federation that `recover` restored is in the list too. This example takes the first one. A wallet that holds more than one federation picks by `id`.

A federation answers a few questions without a network call: [`fed.id()`](./api-federation-id.md), [`fed.name()`](./api-federation-name.md), [`fed.network()`](./api-federation-network.md), [`fed.inviteCode()`](./api-federation-invitecode.md) and [`fed.capabilities()`](./api-federation-capabilities.md). [`fed.ecash()`](./api-federation-ecash.md), [`fed.lightning()`](./api-federation-lightning.md) and [`fed.onchain()`](./api-federation-onchain.md) return `undefined` when the federation does not run that module. [`sdk.federations()`](./api-sdk-federations.md) lists the federations open right now.

::: tip Check
After a successful join, `await fed.balance()` returns `0n` and does not throw. That is the smallest proof that the federation is open, and it is where [Show a balance](./balance.md) begins.
:::

## What you just did {#what-you-just-did}

You read an invite code on the device, showed the user who is behind it and which network it is on, and joined. On every later launch, `federationOnLaunch` brings the same federation back, with no saved invite.

Two things this page did not do. It did not leave a federation: `sdk.forgetFederation(id)` does that, and only for a federation with no money and no pending operation. And a wallet restored from seed words should not use `join` at all. It uses `recover`, which looks for money the seed already holds. [Restore from seed words](./restore.md) shows it.
