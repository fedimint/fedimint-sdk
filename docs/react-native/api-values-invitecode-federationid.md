---
title: 'federationId()'
description: 'Reads the federation id from the invite code.'
---

# InviteCodeLike.federationId

<!-- signature: InviteCodeLike['federationId'] -->

```ts
federationId(): FederationId
```

Reads the `federation` id from the invite `code`. It makes no network call. Synchronous.

**Parameters.** None.

**Returns.** `FederationId`. The id of the `federation` that the `code` names.

**Throws.** Not verified.

**Example**
{#example}

```ts [example.ts]
import type { InviteCodeLike } from '@fedimint/react-native'

declare const invite: InviteCodeLike

const id = invite.federationId() // [!code focus]
console.log(id)
```

**Used in.** [Quickstart, step 02](./quickstart.md#step-02), [Join, step 01](./join.md#step-01)
