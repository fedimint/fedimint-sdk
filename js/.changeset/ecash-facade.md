---
'@fedimint/core': major
---

Implement the ecash facade (Task T7), providing quote-then-send out-of-band note spending, deterministic redemptions, cancellation handling, and operation log backfilling.

Breaking Change: `EcashReceiveDetails.notes` is now `Option<Notes>` (previously `Notes`) to account for backfilled reissuances where original note strings are not stored in upstream operation logs.
