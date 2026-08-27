# @fedimint/core-web (deprecated)

> **Deprecated** – this package is now a thin compatibility shim that re-exports everything from [`@fedimint/core`](https://www.npmjs.com/package/@fedimint/core).

We renamed the package to better reflect that the codebase supports multiple JavaScript runtimes, not only browsers. All new development happens under `@fedimint/core`.

## Migration

Update your dependencies to point at the new package:

```bash
pnpm add @fedimint/core
# or npm install @fedimint/core, yarn add @fedimint/core, etc.
```

After updating, change your imports accordingly:

```ts
- import { xyz } from '@fedimint/core-web'
+ import { xyz } from '@fedimint/core'
```

## Temporary shim

This package re-exports `@fedimint/core` and logs a one-time warning at runtime. It keeps existing projects running while you migrate, but it will be removed in a future release.

If you rely on it, please schedule the migration soon.
