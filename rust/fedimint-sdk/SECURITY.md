# Security invariants

What this crate has to keep true to stay safe, written for whoever changes the code involved. Read
it before changing how a storage location is opened, claimed or closed, anything that keeps a
store open in the background, or a storage backend.

## One opener per storage location

Two writers over one wallet's state can corrupt it and double-spend notes. A storage location is
therefore open in at most one place at a time: one instance, in one process, browser tab or
worker. A second open is refused with `StorageInUse` straight away. It never waits for the opener
in its way, and there is no override.

`Storage::open` in `src/storage.rs` is the only production code that opens a location's store.
`db::open_native_root` opens one without a claim, and exists only in tests.

### Native

- The claim is an exclusive advisory lock on a `LOCK` file in the location, taken without
  waiting, before the embedded store is opened.
- The embedded store (`fedimint-rocksdb` through `fedimint-db-locked`) takes a lock of its own,
  `db.db.lock`, and waits for it with no timeout and no error. The claim is what keeps an opener
  from reaching that wait while the store is open somewhere else. An opener that reaches it hangs
  for as long as the other store stays open (#406).
- The claim lives inside the store. `ClaimedStore` holds the store and then the claim, and fields
  drop in declaration order, so the store has closed before the claim is released. There is no
  moment when a location looks free while its store is still open.
- The kernel releases the lock when the process dies, so a crashed process never leaves a stale
  claim behind, and `StorageInUse` always means a live opener.

### Browser

- There is no lock file. The claim is the file's sync access handle in the origin-private file
  system, which the browser grants to one tab, worker or iframe at a time. A refused handle is
  reported as `StorageInUse`.
- The handle lives inside the store, but the store does not close it when dropped:
  `fedimint-cursed-redb` at the pinned revision never closes it. It is released when the worker
  that opened it ends. The web package hosts the SDK in a dedicated worker and terminates that
  worker on `close()`, which is what hands a location back in a browser.

## How long a claim lasts

`SdkBuilder::build` takes the claim in its first step. It is held until the store closes, which is
when the last `Database` clone over it is dropped. Everything built on an instance holds one: the
`Sdk`, every `Federation`, every operation handle and subscriber taken from a federation, and the
Fedimint client under each federation, background tasks included.

- `Sdk::shutdown` does not release the claim. It stops the instance's background work, so that
  the store closes as soon as the application drops its last handle. Reopening a location in the
  same process therefore means shutting down, dropping everything, then opening.
- Without `Sdk::shutdown`, dropping the last handle drops each Fedimint client. On a multi-thread
  Tokio runtime the client shuts down inline. Anywhere else, such as a current-thread runtime or
  a thread with no runtime at all, which is where a foreign binding frees its objects, the client
  only tells its executor to stop, and the executor holds the store until it exits. A reopen in
  the meantime is refused.
- The SDK's own background work holds the instance weakly, or stops with the client's task group,
  which `Sdk::shutdown` shuts down. It never keeps a location claimed on its own.
- Opening survives cancellation. The claim is taken and the store opened in one blocking task,
  which runs to completion even when `build` is dropped. A dropped `build` leaves a store that
  closes before its claim is released, never a free claim over a store still being opened.
- When `build` fails after opening, the store is dropped along with it, and the location comes
  free by the rules above.

In-memory storage takes no claim. Each `Storage::in_memory()` value is its own store, so it can
never be opened twice.

## What it does not protect against

Copying a location's contents elsewhere and opening both copies. That is the same mistake as
restoring one wallet's backup onto two devices, and nothing here can detect it.

## When to revisit this

- Replacing a storage backend, or changing how one is opened. The claim has to be taken before
  the backend can wait on a lock of its own, has to fail fast, and has to be released only once
  the backend has closed.
- Splitting the claim and the open across more than one task or `await`. That reopens a window in
  which a cancelled open leaves a free claim over a store that is still open.
- Adding anything that holds a `Database` clone outside the application's handles, such as a new
  background task. It keeps every claim it outlives, so it has to stop on shutdown or hold the
  instance weakly.
- Adding a storage kind, which needs its own answer to what enforces a single opener.
- Bumping Fedimint across a change to how a client shuts down when dropped, or to whether
  `fedimint-cursed-redb` closes its handle when dropped. Either changes when a location comes free.

These tests pin the invariant down:

- `storage::tests::a_second_opener_is_refused_and_the_claim_returns_when_the_store_closes`
- `sdk::tests::building::a_location_is_refused_while_the_instance_on_it_is_still_held`
- `sdk::tests::building::a_federation_handle_holds_the_location_on_its_own`
