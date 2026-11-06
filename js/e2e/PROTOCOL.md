# Harness protocol, version 1

Every platform's SDK harness speaks this protocol, so one suite (`js/e2e`) tests the
SDK on all of them. A harness is a debug-only app or page with a three-element
console and a dispatcher that maps each method below onto that platform's generated
bindings. [`src/protocol`](src/protocol) is the machine-checked copy of this file:
the suite decodes every answer against it. Keep the two in step.

Harnesses: Android [`ConsoleActivity`](../../android/app/src/debug/java/org/fedimint/demo/harness/ConsoleActivity.kt).

## The console

| id         | element        | rules                                                                                           |
| ---------- | -------------- | ----------------------------------------------------------------------------------------------- |
| `request`  | editable text  | the driver writes one request; the harness empties the field when it takes it                   |
| `run`      | button         | takes the request and, before returning, shows `{"v":1,"id":<id>,"pending":true}` in `response` |
| `response` | read-only text | exactly one JSON value, nothing else; `{"v":1,"ready":true}` at launch                          |

- **One call at a time.** A request while another runs is answered `HARNESS_BUSY`.
- **Retries are safe.** A request whose `id` equals the last one taken is answered
  from the cache (pending, or its final answer). `run` with an empty `request` does
  nothing.
- **Locators:** Android resource-id `org.fedimint.demo:id/<id>`; iOS accessibility
  identifier `<id>`; web element id `<id>`.

## Envelope

```
-> {"v":1,"id":"k3f9:17","method":"lightning.receive","params":{"amountMsats":30000,"description":"x"}}
<- {"v":1,"id":"k3f9:17","ok":true,"result":{"invoice":"lnbcrt…","operationId":"…"},"ms":412}
<- {"v":1,"id":"k3f9:17","ok":false,"error":{"code":"INVALID_INPUT","reason":"…"},"ms":3}
<- {"v":1,"id":null,"ok":false,"error":{"code":"HARNESS_BAD_REQUEST","reason":"…"},"received":"<first 200 chars>"}
```

- A request is one line of JSON with every non-ASCII character escaped as `\uXXXX`.
  `params` may be omitted for `{}`.
- The driver makes ids unique per run (`<run nonce>:<seq>`), so an answer left on
  screen from an earlier run never matches.
- `ms` is how long the call took inside the harness.

## Values

- **Keys** are the camelCase of the Rust field, with a unit suffix: `…Msats` for
  `Amount`, `…Sats` for `Sats`, `…Ms` for `Timestamp` (epoch milliseconds).
- **Numbers** are JSON numbers. A value above 2^53−1 is answered `HARNESS_INTERNAL`.
- **Enums** (`ErrorCode`, `OperationKind`, `OperationSupport`, `Network`,
  `Direction`, `ActivityStatus`) are the SCREAMING_SNAKE of the Rust variant:
  `INVALID_INPUT`, `LN_RECEIVE`, `REGTEST`.
- **States and routes** are always objects: `{"type":"<SCREAMING_SNAKE>", …fields}`,
  fieldless ones included (`{"type":"CANCELED"}`). The states of each kind are
  listed in [`values.ts`](src/protocol/values.ts) (`STATES`).
- **Absent values** are `null`, never an omitted key.
- **Ids, invoices, addresses, txids and preimages** are strings as the SDK renders
  them.

## Errors

`error` is `{"code", "reason"}`. An SDK error carries its `ErrorCode` and `reason()`.
Anything else is the harness's own:

| code                     | when                                                                   |
| ------------------------ | ---------------------------------------------------------------------- |
| `HARNESS_BAD_REQUEST`    | the request is not a valid envelope (`id` is null if it can't be read) |
| `HARNESS_UNKNOWN_METHOD` | no such method                                                         |
| `HARNESS_BAD_PARAMS`     | a param is missing or of the wrong type (checked before any state)     |
| `HARNESS_STATE`          | no open SDK, no attached federation, a missing module, already open    |
| `HARNESS_UNKNOWN_REF`    | no such quote, notes or tracked operation                              |
| `HARNESS_TIMEOUT`        | `operation.awaitFinal` ran past its `timeoutMs`                        |
| `HARNESS_BUSY`           | another call is running                                                |
| `HARNESS_INTERNAL`       | any other failure; `reason` names the exception                        |

## What the harness holds

- One `Sdk` (`sdk.open`) and the attached `Federation` (`sdk.join`, or the first one
  `sdk.open` finds). Attaching another drops everything below.
- **Quotes** under `q<n>`, kept after `send`, so sending one again reaches the SDK.
- **Notes** under `n<n>`, from `ecash.send`. `ecash.receive {notesRef}` parses
  `display()` of the held notes, so the string round trip still runs, without a
  multi-kilobyte value crossing the console.
- **Operations** by operation id. Every operation the harness creates, finds through
  `deposits()` or looks up gets one watcher over `updates()`, recording each state
  it delivers until the subscription ends (`final`) or fails (`error`).
- **The balance**: on attach, one `balance()`, then every value `balanceUpdates()`
  delivers (`updates` counts those).
- **Deposits**: the first `onchain.receive` opens `deposits()` before calling
  `receive()`, and follows it for the federation's lifetime.

## Methods

Params are listed as `name: type`; `?` marks an optional one.

| method                       | params                             | result                                                                                                                                                                                                 |
| ---------------------------- | ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `harness.describe`           |                                    | `{protocolVersion: 1, platform, sdkVersion \| null, methods: [string], features: [string]}`                                                                                                            |
| `harness.state`              |                                    | `{sdkOpen, federationId \| null, operations, quotes}`                                                                                                                                                  |
| `harness.echo`               | `payload: string`                  | `{payload}`                                                                                                                                                                                            |
| `sdk.open`                   | `mnemonic?: [string] \| null`      | `{federationIds: [id], attachedFederationId \| null}` — `createFedimintSdk`, then `federations()`                                                                                                      |
| `sdk.mnemonic`               |                                    | `{words: [string]}` — `exportMnemonic().words()`                                                                                                                                                       |
| `invite.parse`               | `inviteCode`                       | `{federationId}` — needs an open SDK on every platform (the web SDK's statics live on its session)                                                                                                     |
| `sdk.preview`                | `inviteCode`                       | `{id, name \| null, network, guardians, modules: [string], meta: {string: string}}`                                                                                                                    |
| `sdk.join`                   | `inviteCode`                       | `{federationId}`; attaches it. `ALREADY_JOINED` is returned as is                                                                                                                                      |
| `federation.info`            |                                    | `{id, name \| null, network, capabilities: {ecash, lightning, onchain}}`                                                                                                                               |
| `federation.balance`         |                                    | `{amountMsats}` — one `balance()` call                                                                                                                                                                 |
| `federation.observedBalance` |                                    | `{amountMsats \| null, updates, error \| null}` — what the stream delivered                                                                                                                            |
| `lightning.receive`          | `amountMsats, description`         | `{invoice, operationId}`                                                                                                                                                                               |
| `lightning.quote`            | `invoice`                          | `{quoteId, invoiceAmountMsats, feeMsats, totalMsats, route, expiresAtMs}`                                                                                                                              |
| `lightning.send`             | `quoteId`                          | `{operationId}`                                                                                                                                                                                        |
| `ecash.quote`                | `amountMsats`                      | `{quoteId, requestedAmountMsats, notesValueMsats, feeMsats, totalMsats, expiresAtMs}`                                                                                                                  |
| `ecash.send`                 | `quoteId`                          | `{operationId, notesRef, notesValueMsats}`                                                                                                                                                             |
| `ecash.receive`              | exactly one of `notes`, `notesRef` | `{operationId, notesValueMsats}`                                                                                                                                                                       |
| `onchain.receive`            |                                    | `{address}`                                                                                                                                                                                            |
| `onchain.deposits`           |                                    | `{operationIds: [id], error \| null}` — the deposits found so far                                                                                                                                      |
| `onchain.quote`              | `address, amountSats`              | `{quoteId, amountSats, feeMsats, totalMsats, expiresAtMs}`                                                                                                                                             |
| `onchain.send`               | `quoteId`                          | `{operationId}`                                                                                                                                                                                        |
| `operation.lookup`           | `operationId`                      | `{found: false}` or `{found: true, kind, support, state \| null}` — `federation.operation(id)`, the typed downcast and `state()`; starts a watcher. `state` is null when the build has no typed handle |
| `operation.observed`         | `operationId`                      | `{kind, states: [state], final, error \| null}` — the watcher's record                                                                                                                                 |
| `operation.awaitFinal`       | `operationId, timeoutMs? = 120000` | `{kind, state}` — `awaitFinal()` on a tracked operation                                                                                                                                                |
| `activity.list`              | `limit? = 20, cursor? \| null`     | `{items: [{operationId, kind, timeMs, amountMsats \| null, feeMsats \| null, direction \| null, status, isFinal}], next \| null}`                                                                      |

## Adding to the protocol

A new method goes in [`methods.ts`](src/protocol/methods.ts) (params and a result
decoder), in this table, and in every harness; a harness lists what it serves in
`harness.describe`, and the runner fails a test that needs a method its target's
harness lacks unless the target says why (`knownUnsupported`). A change to an
existing method's shape bumps the version.
