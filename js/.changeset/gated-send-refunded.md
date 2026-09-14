---
'@fedimint/core': patch
---

Gate `Refunded` state emission on primary-module input recovery settlement for rejected send funding, emit `Failed` if restoration cannot be proven clean, and update transfer terms and rustdoc to be outcome-neutral (#370, #371).
