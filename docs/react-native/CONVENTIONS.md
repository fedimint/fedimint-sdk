# Writing the React Native pages: conventions

The guide teaches a React Native developer to build a wallet with upstream's npm packages.
The reader is a human or an agent. Both follow it from start to end, with no help.

## Packages and version

- Install both: `@fedimint/react-native` and `@fedimint/react-native-bindings`. The app must
  list both, because autolinking links only packages the app names directly.
- Import everything from `@fedimint/react-native`. It re-exports the bindings.
- The version is in `react-native/VERSION`. Write it literally (today `0.1.0-beta.2`). `tools/set_version.py`
  replaces every occurrence. Never write `latest`, `beta`, `^` or `~`.
- Never mention any other wrapper package or a vendored or locally built copy.

## The API (proved: the fixtures compile)

| Task                | Call                                                                                                                                                         |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Open                | `const session = await openSdk({ dataDir, mnemonic? })`; `session.sdk`; `await session.close()`                                                              |
| dataDir             | A plain path, not a `file://` URI. Strip `file://` if your file library returns one                                                                          |
| Seed words          | `session.sdk.exportMnemonic().words()` (sync)                                                                                                                |
| Parse invite        | `InviteCode.parse(code.trim())` (throws InvalidInput; no network). `.federationId()`, `.display()`                                                           |
| Preview             | `await sdk.preview(invite)` → `{ id, name?, network, guardians, modules, meta }`                                                                             |
| Join                | `await sdk.join(invite)` → `FederationLike`. AlreadyJoined if held                                                                                           |
| Reconnect on launch | `sdk.federation(id) ?? (stored ? await sdk.reopenFederation(id) : await sdk.join(invite))`; stored = `sdk.storedFederations().some(f => f.id === id)`        |
| Federation          | `id()`, `name()`, `network()`, `inviteCode().display()`, `balance()`, `balanceUpdates()`, `ecash()`, `lightning()`, `onchain()` (methods, may be undefined)  |
| Ecash send          | `const q = await ecash.quote(msat)`; `q.total()`, `q.fee()`; `const { notes, operation } = await ecash.send(q)`; string = `notes.display()`                  |
| Take back           | `await operation.requestCancel()` → final `EcashSendState.Canceled`. Plain enum: compare with `===`                                                          |
| Ecash receive       | `Notes.parse(str).value()` to inspect; `await ecash.receive(Notes.parse(str))`                                                                               |
| LN receive          | `const { invoice, operation } = await ln.receive(msat, description)` (description required)                                                                  |
| LN pay              | `const q = await ln.quote(invoice)`; `q.invoiceAmount()`, `q.fee()`, `q.total()`; `await ln.send(q)`                                                         |
| Observe             | `op.updates()` → `next({ signal })`: first value is the current state; `undefined` = ended. `op.awaitFinal()`. A failed payment is a state, not a throw      |
| Hooks               | `useBalance(federation, onError?)`, `useOperation(operation, onError?)`. No provider. Keyed on `id()`. No retry: remount with a `key` after FederationClosed |
| History             | `await fed.activity(cursor \| undefined, limit)` → `{ items, next? }`, newest first; `fed.operation(id)` → `kind()`, `asEcashSend()`…                        |
| Restore             | `openSdk({ dataDir, mnemonic: words })` over an empty dir; `await sdk.recover(invite)` → `{ federation, progress }`; `sdk.resumeRecovery(id)`                |
| Errors              | Thrown errors are NOT `instanceof Exception`. Use `Exception.hasInner(e)` then `Exception.getInner(e).code()` / `.reason()`; name = `ErrorCode[code]`        |
| Amounts             | `Amount = bigint` in msat. sats → msat: `BigInt(sats) * 1000n`. Print: `${msat / 1000n} sats`. On-chain uses `Sats` (whole sats). No helpers ship            |

Terminal states: ecash receive Done/Failed; ecash send Redeemed/Canceled; LN receive
Claimed/Canceled/Expired/Failed; LN pay Success/Refunded/Failed; recovery Done/Failed.
States with data are tagged unions: compare `state.tag` with `*_Tags.X`.

## Facts for setup

- React >= 18, React Native >= 0.78 (0.83 recommended), New Architecture required.
- iOS 15.0+, Android API 24+. Android ABIs: arm64-v8a and x86_64 only. Set
  `reactNativeArchitectures=arm64-v8a,x86_64` in `android/gradle.properties`.
- The packages ship prebuilt native libraries. Nothing compiles at install.
- Expo: not in Expo Go. Add `"plugins": ["@fedimint/react-native"]` to app.json (Expo SDK 52+),
  then `npx expo prebuild` and a development build. The plugin sets AndroidX and New
  Architecture on Android; it does nothing on iOS.
- Test federation: Fedi Testnet, on signet. Its invite code:
  `fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75`
  Pay it from a signet faucet. (No faucet URL is confirmed yet.)

## Page rules

- Pages are VitePress Markdown, one file per page. `nav.json` orders them; the sidebar and
  `llms.txt` both read it. A new page needs an entry there.
- Every `##` heading has an anchor: `## Title {#id}`. Every step is a heading
  `### NN · Title {#step-NN}`. Renumber steps after a cut. The fixture path comes from the
  anchor.
- Every fence says what it is:
  - ` ```ts [path] `: a file. It is extracted to `fixtures/react-native/<page>/step-NN/path`
    and type-checked.
  - ` ```text no-check ` (any language): output or a command. Not checked.
  - `<!-- signature: T -->` on the line before a ` ```ts ` fence: a call signature, checked
    against the real type `T`.
    A fence with none of these fails the extractor.
- `// [!code focus]` at the end of a line focuses it and blurs the rest. The extractor removes it.
- Use VitePress containers: `::: tip Check` for what the reader will see, `::: warning` and
  `::: info` for callouts. Use `<Badge type="tip|danger" text="..."/>` in the `cap-row`.
- No em dash or en dash.
- Every code block compiles as pasted. From `docs/`, run
  `python3 tools/extract_snippets.py && (cd fixtures/react-native && npm run check)`.
- Every step ends with a check the reader can see ("You will see ...").
- The cap-row states only what is true of these packages.
