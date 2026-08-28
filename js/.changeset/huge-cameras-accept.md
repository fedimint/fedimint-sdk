---
'@fedimint/types': patch
'@fedimint/core': patch
---

patch: fix parseInviteCode, parseBolt11Invoice and previewFederation. - Added detailed type definitions for `PreviewFederation` including `JsonClientConfig` structure - Updated `PreviewFederation.config` to return properly typed `JsonClientConfig` instead of generic object - Added type definitions for `GlobalClientConfig`, `PeerUrl`, `CoreConsensusVersion`, and `ModuleConfig`
