# @fedimint/transport-web

## 0.1.3

### Patch Changes

- 516e51b: Upgrade TypeScript to 6.0.3 and fix resulting deprecations
- 516e51b: - Initial Release of react-native for Fedimint-SDK
  - Updated WalletDirector to Accept Path for react-native
  - Bumped packages to match versions across the monorepo
- 7002c28: Fail pending RPC requests when the transport crashes instead of hanging forever.

  Uncaught errors and unhandled rejections in the wasm worker (e.g. a panic in the wasm
  client) previously went nowhere: the request that triggered them never resolved and
  callers were left to hit their own timeouts with no error message. The worker now
  reports such crashes as transport-level errors and `TransportClient` rejects all
  in-flight requests with the underlying error.

- 9f57202: Include the error's stack when reporting uncaught wasm worker errors.

  `event.message` alone drops the stack, and for a wasm trap the stack (with its wasm frame
  references) is the only clue to what crashed.

- Updated dependencies [b43a924]
- Updated dependencies [299e79b]
- Updated dependencies [516e51b]
- Updated dependencies [516e51b]
- Updated dependencies [bdba63f]
- Updated dependencies [48288a9]
- Updated dependencies [69fdcb1]
- Updated dependencies [abd43e0]
- Updated dependencies [c65cc13]
- Updated dependencies [cf43f91]
- Updated dependencies [82a1863]
- Updated dependencies [33e5de2]
- Updated dependencies [7002c28]
- Updated dependencies [1744c92]
  - @fedimint/core@0.2.0
  - @fedimint/fedimint-client-wasm-bundler@0.1.2
  - @fedimint/types@0.0.4

## 0.1.2

### Patch Changes

- Updated dependencies [ba37695]
  - @fedimint/fedimint-client-wasm-bundler@0.1.1

## 0.1.1

### Patch Changes

- c04230a: Bump wasm to redb supported version [commit](https://github.com/fedimint/fedimint/tree/a88f7f6ceb988ee964bf06900183c3c16f7f4c38)
- 6c07908: Bump all the deps with taze.
- Updated dependencies [c04230a]
- Updated dependencies [6c07908]
  - @fedimint/types@0.0.3
  - @fedimint/core@0.1.3

## 0.1.0

### Minor Changes

- fdfc947: Rename @fedimint/core-web to @fedimint/core

### Patch Changes

- adfc30a: Split transport into external package from core-web. Externalize shared types.
- Updated dependencies [adfc30a]
- Updated dependencies [fdfc947]
  - @fedimint/types@0.0.2
  - @fedimint/core@0.1.1
