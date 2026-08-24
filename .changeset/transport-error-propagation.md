---
'@fedimint/transport-web': patch
'@fedimint/core': patch
---

Fail pending RPC requests when the transport crashes instead of hanging forever.

Uncaught errors and unhandled rejections in the wasm worker (e.g. a panic in the wasm
client) previously went nowhere: the request that triggered them never resolved and
callers were left to hit their own timeouts with no error message. The worker now
reports such crashes as transport-level errors and `TransportClient` rejects all
in-flight requests with the underlying error.
