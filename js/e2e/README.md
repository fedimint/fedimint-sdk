# @fedimint/e2e

One device-level test suite for the Fedimint SDK on every platform it ships on. The
tests are written once and run against a small **harness** each platform builds over
its generated bindings: a debug-only console that takes a JSON request, calls the SDK
and shows a JSON answer, as [PROTOCOL.md](PROTOCOL.md) defines. What this adds over
compiling the bindings is a running device: the native library is loaded and the
calls execute against a real devimint federation.

| target    | harness                                           | driven by            | CI                     |
| --------- | ------------------------------------------------- | -------------------- | ---------------------- |
| `android` | `android/app/src/debug/…/harness/ConsoleActivity` | Appium, UiAutomator2 | `android-e2e.yml`      |
| `web`     | `js/web/sdk-harness` (served by `vite preview`)   | Playwright, Chromium | `verify.yml` `web-e2e` |

A target may list tests it cannot run yet in its `knownUnsupported`, with the reason;
they are reported as `SKIPPED`, never silently dropped. The web target lists `errors`
today: a malformed invoice there throws an internal error instead of `INVALID_INPUT`.

## Running

```bash
just test-e2e android        # a v1 federation (mint, wallet, ln)
just test-e2e android v2     # a v2 federation (mintv2, walletv2, lnv2)
just test-e2e web v2
```

That builds the harness, then, inside the target's nix shell, starts devimint
(`scripts/setup_test_shell.sh`) and runs [`scripts/e2e/run.sh`](../../scripts/e2e/run.sh),
which provisions the device and runs [`src/runner.ts`](src/runner.ts). Building and
running are separate on purpose: a Gradle build next to an emulator and a federation
starved the emulator. `TESTS_TO_RUN` picks tests (names, or `all`); CI runs all.
Screenshots, UI trees and logs land in `.artifacts/<target>-<shape>/`.

## The tests

In the order `all` runs them. Each also runs alone: fixtures build what it needs from a
fresh install.

| name               | what it proves                                                                                                                                                                             |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `console`          | the harness speaks PROTOCOL.md: it names its platform, serves every method, carries 1.5 KB of quotes, backslashes and non-ASCII both ways intact, and answers bad calls with harness codes |
| `mnemonic`         | the SDK opens over the harness's storage and exports a 12- or 24-word mnemonic (no federation)                                                                                             |
| `inviteCode`       | `InviteCode.parse` gives the federation id; a malformed code is `INVALID_INPUT` (no federation)                                                                                            |
| `federation`       | the join: regtest, all three capabilities, this shape's module kinds and none of the other's, one id by every path, an empty balance                                                       |
| `lightning`        | lightning in: devimint pays a harness invoice, the receive is `CLAIMED`, and `balanceUpdates()` delivers a rise of at most the invoice                                                     |
| `mint`             | ecash out and back in: the debit is exactly the quote, the reissue is `DONE`, and the send's record is `ECASH_SEND` and not `CANCELED`                                                     |
| `lightningSend`    | lightning out: the payee reports the invoice paid, the debit is exactly the quoted total, the preimage hashes to the payment hash; the spent quote is then `QUOTE_EXPIRED`                 |
| `errors`           | `INVALID_INPUT` for bad notes, a bad invoice and a zero amount, `INSUFFICIENT_BALANCE` for an oversized invoice with no quote left behind; no balance or history change                    |
| `persistence`      | a restart keeps the seed, reattaches the federation on open, and keeps the balance                                                                                                         |
| `operationRestart` | an unpaid receive survives a restart: found by id through `federation.operation`, still waiting, then followed to `CLAIMED`                                                                |
| `onchain`          | a deposit is claimed with its txid, gross and net, crediting exactly the net; a withdrawal `SUCCEEDED`, reaches and confirms at its address, and debits exactly the quoted total           |

### Where the generations differ

The tests assert what each generation documents rather than identical behaviour:

- **Fees.** lnv2 and mintv2 charge per transaction, so received amounts are bounded,
  not pinned, and every debit is checked against the SDK's own quote. mintv2 rounds
  ecash to multiples of 512 msat and the SDK only sends notes it holds in exact
  denominations (`NOT_SUPPORTED` otherwise), so `mint` walks a short list of amounts.
- **Ecash reclaim.** Neither generation has to report a redeemed send as `REDEEMED`
  at once, so `mint` requires only that it is not `CANCELED`.
- **lnv2 gateway.** The lnv2 client picks a gateway at random and only devimint's LND
  gateway is funded, so on v2 `scripts/e2e/lib.sh` removes the others from every
  guardian's list. On v1 the SDK picks the cheapest, which is LND.
- **DNS check (Android).** The SDK falling back to public DNS fails the run; the
  "Got DNS servers" trace is required only on v1, where `lightning` dials devimint's
  iroh gateway.

### What comes from devimint

Everything `devimint wasm-test-setup` exports: `FAUCET` (invite code, paying and
issuing invoices), `FM_LNCLI` (paying from LND, decoding invoices), `FM_GWCLI_LDK`
(whether the payee was paid), `FM_BTC_CLIENT` (funding addresses, mining, checking
withdrawals), `FM_MINT_CLIENT` (the lnv2 pin), and the ports a target forwards. A
missing one fails the test that needs it, naming it.

## Layout

```
src/protocol   the protocol as types and decoders (PROTOCOL.md is the prose)
src/client     HarnessClient: request ids, retries, timeouts, decoding
src/backends   one per automation technology: write request, tap run, read response
src/targets    one per platform: session, restart, reset, failure capture, log checks
src/env        devimint, the faucet, bitcoind, lightning, the federation shape
src/fixtures   build a state from a fresh install: walletOpen, joinedFederation, funded
src/flows      shared steps (fund over lightning, wait for a balance or a state)
src/tests      the tests, one file each, listed in src/registry.ts
```

## Writing a test

1. Add `src/tests/<name>.test.ts` exporting a `TestDef`: its `prerequisites` (built by
   fixtures), the states it `produces`, the methods it `requires`, and `run`, which
   throws to fail. Register it in [`src/registry.ts`](src/registry.ts).
2. Call the harness with `h.call(method, params)` (throws on an error answer) or
   `h.try` (returns it); wait on recorded state with `h.poll` or the helpers in
   `src/flows`.
3. Assert outcomes: the structured error `code`, the operation's final state, the
   exact balance change against the SDK's own quote, and the other side of a payment
   where there is one. Every wait has a timeout.
4. Read the federation's generation with `currentShape()` where v1 and v2 differ.

## Adding a platform

1. A harness: the three-element console and a dispatcher serving every method in
   PROTOCOL.md, with exhaustive encoders so a new SDK variant breaks its build.
2. A `Target` in `src/targets` and its provisioning in `scripts/e2e/<target>.sh`.
3. A backend in `src/backends` only for a new automation technology.

Run `console` first: it checks the harness against the protocol before anything else.
