---
'@fedimint/transport-web': patch
---

Include the error's stack when reporting uncaught wasm worker errors.

`event.message` alone drops the stack, and for a wasm trap the stack (with its wasm frame
references) is the only clue to what crashed.
