---
title: 'preview()'
description: 'Looks up a federation from its invite code.'
---

# SdkLike.preview

<!-- signature: SdkLike['preview'] -->

```ts
preview(invite: InviteCodeLike): Promise<FederationPreview>
```

Looks up a `federation` from its invite `code`. It joins nothing and stores nothing.

**Parameters**

| Name     | Type             | Description                   |
| -------- | ---------------- | ----------------------------- |
| `invite` | `InviteCodeLike` | The invite `code` to look up. |

**Returns.** `FederationPreview`. The name (optional), network, number of guardians, module names and metadata of the `federation`.

**Throws.** [`FederationUnreachable`](./errors.md#code-federationunreachable), [`Timeout`](./errors.md#code-timeout), [`UnsupportedFederation`](./errors.md#code-unsupportedfederation), [`FederationClosed`](./errors.md#code-federationclosed).

**Example**
{#example}

```ts [example.ts]
import type { InviteCodeLike, SdkLike } from '@fedimint/react-native'

declare const sdk: SdkLike
declare const invite: InviteCodeLike

const preview = await sdk.preview(invite) // [!code focus]
console.log(preview.name ?? 'unnamed federation')
```

**Used in.** [Join, step 02](./join.md#step-02), [Errors, step 01](./errors.md#step-01)
