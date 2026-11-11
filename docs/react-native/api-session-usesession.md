---
title: 'useSession()'
description: 'Gives the session from the nearest SessionProvider.'
---

# useSession

<!-- signature: typeof import('../../session/step-02/src/SessionContext').useSession -->

```ts
useSession(): SdkSession
```

Gives the session from the nearest `SessionProvider`. This is guide code from the Session page. The package does not export it.

**Parameters.** None.

**Returns.** `SdkSession`. The session from the nearest `SessionProvider`.

**Throws.** A plain Error when no SessionProvider is above the hook. It carries no ErrorCode.

**Example**
{#example}

```tsx [example.tsx]
import type { SdkSession } from '@fedimint/react-native'

declare function useSession(): SdkSession // from the Session page

const session = useSession() // [!code focus]
console.log(session.sdk.federations().length)
```

**Used in.** [Session, step 02](./session.md#step-02)
