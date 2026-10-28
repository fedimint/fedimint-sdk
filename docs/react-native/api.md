---
title: 'Overview'
description: 'Every method the guides use, in groups, with a link to a page for each.'
---

# API

Every method the guides use: signature, parameters, return value and errors.
{.subtitle}

This section has one page for each method that the guides call. Each page shows the signature, the parameters, the return value, the error codes it can throw, an example, and the guide steps that use it.

Many calls in the bindings take a last optional parameter, `asyncOpts_`, for an abort signal. The signatures on this page leave it out.

`useSession` is not part of the package. It is guide code from [the Session page](./session.md#step-02).

## Session {#session}

Open the SDK, close it, and follow it with hooks.

| Method                                          | Signature                                                                                                           |
| ----------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| [`openSdk`](./api-session-opensdk.md)           | `openSdk(options: OpenOptions): Promise<SdkSession>`                                                                |
| [`SdkSession.close`](./api-session-close.md)    | `close(): Promise<void>`                                                                                            |
| [`useSession`](./api-session-usesession.md)     | `useSession(): SdkSession`                                                                                          |
| [`useBalance`](./api-session-usebalance.md)     | `useBalance(federation: FederationLike \| undefined, onError?: (err: unknown) => void): Amount \| undefined`        |
| [`useOperation`](./api-session-useoperation.md) | `useOperation<S>(operation: ObservableOperation<S> \| undefined, onError?: (err: unknown) => void): S \| undefined` |

## Sdk {#sdk}

Methods of SdkLike, the type of `session.sdk`.

| Method                                                        | Signature                                                               |
| ------------------------------------------------------------- | ----------------------------------------------------------------------- |
| [`SdkLike.preview`](./api-sdk-preview.md)                     | `preview(invite: InviteCodeLike): Promise<FederationPreview>`           |
| [`SdkLike.join`](./api-sdk-join.md)                           | `join(invite: InviteCodeLike): Promise<FederationLike>`                 |
| [`SdkLike.recover`](./api-sdk-recover.md)                     | `recover(invite: InviteCodeLike): Promise<RecoveryHandle>`              |
| [`SdkLike.resumeRecovery`](./api-sdk-resumerecovery.md)       | `resumeRecovery(id: FederationId): Promise<RecoveryHandle>`             |
| [`SdkLike.recoveryStatus`](./api-sdk-recoverystatus.md)       | `recoveryStatus(id: FederationId): Promise<RecoveryState \| undefined>` |
| [`SdkLike.federation`](./api-sdk-federation.md)               | `federation(id: FederationId): FederationLike \| undefined`             |
| [`SdkLike.federations`](./api-sdk-federations.md)             | `federations(): Array<FederationLike>`                                  |
| [`SdkLike.storedFederations`](./api-sdk-storedfederations.md) | `storedFederations(): Array<FederationInfo>`                            |
| [`SdkLike.reopenFederation`](./api-sdk-reopenfederation.md)   | `reopenFederation(id: FederationId): Promise<FederationLike>`           |
| [`SdkLike.exportMnemonic`](./api-sdk-exportmnemonic.md)       | `exportMnemonic(): MnemonicLike`                                        |

## Federation {#federation}

Methods of FederationLike, the handle you get from `join`, `federation` and reopenFederation.

| Method                                                                | Signature                                                                     |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| [`FederationLike.id`](./api-federation-id.md)                         | `id(): FederationId`                                                          |
| [`FederationLike.name`](./api-federation-name.md)                     | `name(): string \| undefined`                                                 |
| [`FederationLike.network`](./api-federation-network.md)               | `network(): Network`                                                          |
| [`FederationLike.balance`](./api-federation-balance.md)               | `balance(): Promise<Amount>`                                                  |
| [`FederationLike.balanceUpdates`](./api-federation-balanceupdates.md) | `balanceUpdates(): BalanceUpdatesLike`                                        |
| [`FederationLike.activity`](./api-federation-activity.md)             | `activity(cursor: Cursor \| undefined, limit: number): Promise<ActivityPage>` |
| [`FederationLike.operation`](./api-federation-operation.md)           | `operation(id: OperationId): Promise<AnyOperationLike \| undefined>`          |
| [`FederationLike.ecash`](./api-federation-ecash.md)                   | `ecash(): EcashLike \| undefined`                                             |
| [`FederationLike.lightning`](./api-federation-lightning.md)           | `lightning(): LightningLike \| undefined`                                     |
| [`FederationLike.capabilities`](./api-federation-capabilities.md)     | `capabilities(): Capabilities`                                                |
| [`FederationLike.inviteCode`](./api-federation-invitecode.md)         | `inviteCode(): InviteCodeLike`                                                |
| [`FederationLike.meta`](./api-federation-meta.md)                     | `meta(): MetaLike`                                                            |
| [`FederationLike.onchain`](./api-federation-onchain.md)               | `onchain(): OnchainLike \| undefined`                                         |

## Ecash {#ecash}

Methods of EcashLike, from `federation.ecash()`, and of the quote it returns.

| Method                                                             | Signature                                                       |
| ------------------------------------------------------------------ | --------------------------------------------------------------- |
| [`EcashLike.quote`](./api-ecash-quote.md)                          | `quote(amount: Amount): Promise<EcashQuoteLike>`                |
| [`EcashLike.send`](./api-ecash-send.md)                            | `send(quote: EcashQuoteLike): Promise<EcashSendHandle>`         |
| [`EcashLike.receive`](./api-ecash-receive.md)                      | `receive(notes: NotesLike): Promise<EcashReceiveOperationLike>` |
| [`EcashQuoteLike.requestedAmount`](./api-ecash-requestedamount.md) | `requestedAmount(): Amount`                                     |
| [`EcashQuoteLike.notesValue`](./api-ecash-notesvalue.md)           | `notesValue(): Amount`                                          |
| [`EcashQuoteLike.fee`](./api-ecash-fee.md)                         | `fee(): Amount`                                                 |
| [`EcashQuoteLike.total`](./api-ecash-total.md)                     | `total(): Amount`                                               |

## Lightning {#lightning}

Methods of LightningLike, from `federation.lightning()`, and of the quote it returns.

| Method                                                          | Signature                                                                |
| --------------------------------------------------------------- | ------------------------------------------------------------------------ |
| [`LightningLike.quote`](./api-lightning-quote.md)               | `quote(invoice: Bolt11Invoice): Promise<LnQuoteLike>`                    |
| [`LightningLike.send`](./api-lightning-send.md)                 | `send(quote: LnQuoteLike): Promise<LnSendOperationLike>`                 |
| [`LightningLike.receive`](./api-lightning-receive.md)           | `receive(amount: Amount, description: string): Promise<LnReceiveHandle>` |
| [`LnQuoteLike.invoiceAmount`](./api-lightning-invoiceamount.md) | `invoiceAmount(): Amount`                                                |
| [`LnQuoteLike.fee`](./api-lightning-fee.md)                     | `fee(): Amount`                                                          |
| [`LnQuoteLike.total`](./api-lightning-total.md)                 | `total(): Amount`                                                        |

## Operation {#operation}

The handles EcashSendOperationLike, EcashReceiveOperationLike, LnReceiveOperationLike, LnSendOperationLike and RecoveryOperationLike share the same state methods. AnyOperationLike is the untyped handle. The signatures below use EcashSendOperationLike. Each other handle uses its own state type in place of EcashSendState.

| Method                                                                     | Signature                                            |
| -------------------------------------------------------------------------- | ---------------------------------------------------- |
| [`EcashSendOperationLike.state`](./api-operation-state.md)                 | `state(): Promise<EcashSendState>`                   |
| [`EcashSendOperationLike.updates`](./api-operation-updates.md)             | `updates(): EcashSendOperationUpdatesLike`           |
| [`EcashSendOperationLike.awaitFinal`](./api-operation-awaitfinal.md)       | `awaitFinal(): Promise<EcashSendState>`              |
| [`EcashSendOperationLike.requestCancel`](./api-operation-requestcancel.md) | `requestCancel(): Promise<void>`                     |
| [`AnyOperationLike.asEcashSend`](./api-operation-asecashsend.md)           | `asEcashSend(): EcashSendOperationLike \| undefined` |
| [`AnyOperationLike.kind`](./api-operation-kind.md)                         | `kind(): OperationKind`                              |
| [`EcashSendOperationLike.details`](./api-operation-details.md)             | `details(): Promise<EcashSendDetails>`               |

## Values {#values}

Parse strings into checked values, and read seed `words`.

| Method                                                                   | Signature                                        |
| ------------------------------------------------------------------------ | ------------------------------------------------ |
| [`InviteCode.parse`](./api-values-invitecode-parse.md)                   | `InviteCode.parse(code: string): InviteCodeLike` |
| [`InviteCodeLike.federationId`](./api-values-invitecode-federationid.md) | `federationId(): FederationId`                   |
| [`Notes.parse`](./api-values-notes-parse.md)                             | `Notes.parse(notes: string): NotesLike`          |
| [`NotesLike.value`](./api-values-notes-value.md)                         | `value(): Amount`                                |
| [`NotesLike.display`](./api-values-notes-display.md)                     | `display(): string`                              |
| [`MnemonicLike.words`](./api-values-mnemonic-words.md)                   | `words(): Array<string>`                         |

## Errors (Exception) {#errors}

Read the `Exception` that the SDK throws. Each `code` of `ErrorCode` is described in the table on the Errors page.

| Method                                             | Signature                                                              |
| -------------------------------------------------- | ---------------------------------------------------------------------- |
| [`Exception.hasInner`](./api-errors-hasinner.md)   | `Exception.hasInner(obj_: any): obj_ is UniffiThrownObject<Exception>` |
| [`Exception.getInner`](./api-errors-getinner.md)   | `Exception.getInner(err: UniffiThrownObject<Exception>): Exception`    |
| [`ExceptionLike.code`](./api-errors-code.md)       | `code(): ErrorCode`                                                    |
| [`ExceptionLike.reason`](./api-errors-reason.md)   | `reason(): string`                                                     |
| [`ExceptionLike.details`](./api-errors-details.md) | `details(): RawErrorDetails \| undefined`                              |
