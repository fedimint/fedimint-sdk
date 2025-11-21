# @fedimint/types

## 0.0.4

### Patch Changes

- 48288a9: patch: fix parseInviteCode, parseBolt11Invoice and previewFederation. - Added detailed type definitions for `PreviewFederation` including `JsonClientConfig` structure - Updated `PreviewFederation.config` to return properly typed `JsonClientConfig` instead of generic object - Added type definitions for `GlobalClientConfig`, `PeerUrl`, `CoreConsensusVersion`, and `ModuleConfig`

## 0.0.3

### Patch Changes

- c04230a: Bump wasm to redb supported version [commit](https://github.com/fedimint/fedimint/tree/a88f7f6ceb988ee964bf06900183c3c16f7f4c38)
- 6c07908: Bump all the deps with taze.

## 0.0.2

### Patch Changes

- adfc30a: Split transport into external package from core-web. Externalize shared types.
