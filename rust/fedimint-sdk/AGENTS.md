# AGENTS.md

Read [`SECURITY.md`](SECURITY.md) before changing how a storage location is opened, claimed or
closed, `Sdk::shutdown`, anything that keeps a store open in the background, or a storage backend.
It describes the single-opener invariant that keeps two writers off one wallet's state, and what
changes in this crate have to preserve it.
