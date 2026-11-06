// The value encodings of js/e2e/PROTOCOL.md for the web SDK, whose fieldless
// enums are numeric and whose tagged states arrive as `{tag, inner}`. Every
// map here is a `Record` over the enum and every state switch ends in a
// `never` check, so a variant added to the SDK fails the typecheck until it is
// encoded.

import {
  ActivityStatus,
  Direction,
  EcashReceiveState_Tags,
  EcashSendState,
  ErrorCode,
  LightningRoute_Tags,
  LnReceiveState_Tags,
  LnSendState_Tags,
  Network,
  OnchainReceiveState_Tags,
  OnchainSendState_Tags,
  OperationKind,
  OperationSupport,
  RecoveryState_Tags,
  SdkError,
} from '@fedimint/web-sdk'

export type Json = Record<string, unknown>

/** A failure of the harness itself, answered with a HARNESS_* code. */
export class HarnessError extends Error {
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message)
  }
}

export const badParams = (message: string) =>
  new HarnessError('HARNESS_BAD_PARAMS', message)

/** The largest integer every platform's JSON reader holds exactly. */
const MAX_SAFE = BigInt(Number.MAX_SAFE_INTEGER)

export function num(value: bigint | number): number {
  if (typeof value === 'number') return value
  if (value < 0n || value > MAX_SAFE) {
    throw new HarnessError('HARNESS_INTERNAL', `${value} is outside 0..2^53-1`)
  }
  return Number(value)
}

/** `value`, or JSON null for the SDK's absent (`undefined`). */
export const orNull = <T>(value: T | undefined | null): T | null =>
  value ?? null

export const ERROR_CODES: Record<ErrorCode, string> = {
  [ErrorCode.InvalidInput]: 'INVALID_INPUT',
  [ErrorCode.AlreadyJoined]: 'ALREADY_JOINED',
  [ErrorCode.UnsupportedFederation]: 'UNSUPPORTED_FEDERATION',
  [ErrorCode.FederationUnreachable]: 'FEDERATION_UNREACHABLE',
  [ErrorCode.InsufficientBalance]: 'INSUFFICIENT_BALANCE',
  [ErrorCode.BalanceNotEmpty]: 'BALANCE_NOT_EMPTY',
  [ErrorCode.PendingOperations]: 'PENDING_OPERATIONS',
  [ErrorCode.GatewayUnavailable]: 'GATEWAY_UNAVAILABLE',
  [ErrorCode.Recovering]: 'RECOVERING',
  [ErrorCode.NotSupported]: 'NOT_SUPPORTED',
  [ErrorCode.UnsupportedOperation]: 'UNSUPPORTED_OPERATION',
  [ErrorCode.SeedMismatch]: 'SEED_MISMATCH',
  [ErrorCode.StorageInUse]: 'STORAGE_IN_USE',
  [ErrorCode.StorageOrphaned]: 'STORAGE_ORPHANED',
  [ErrorCode.QuoteExpired]: 'QUOTE_EXPIRED',
  [ErrorCode.QuoteChanged]: 'QUOTE_CHANGED',
  [ErrorCode.AmountlessInvoice]: 'AMOUNTLESS_INVOICE',
  [ErrorCode.NetworkMismatch]: 'NETWORK_MISMATCH',
  [ErrorCode.FederationClosed]: 'FEDERATION_CLOSED',
  [ErrorCode.Timeout]: 'TIMEOUT',
  [ErrorCode.Entropy]: 'ENTROPY',
  [ErrorCode.Storage]: 'STORAGE',
  [ErrorCode.Internal]: 'INTERNAL',
}

export const OPERATION_KINDS: Record<OperationKind, string> = {
  [OperationKind.EcashSend]: 'ECASH_SEND',
  [OperationKind.EcashReceive]: 'ECASH_RECEIVE',
  [OperationKind.LnSend]: 'LN_SEND',
  [OperationKind.LnReceive]: 'LN_RECEIVE',
  [OperationKind.OnchainSend]: 'ONCHAIN_SEND',
  [OperationKind.OnchainReceive]: 'ONCHAIN_RECEIVE',
  [OperationKind.Recovery]: 'RECOVERY',
  [OperationKind.Unknown]: 'UNKNOWN',
}

export const OPERATION_SUPPORT: Record<OperationSupport, string> = {
  [OperationSupport.Observable]: 'OBSERVABLE',
  [OperationSupport.UnknownKind]: 'UNKNOWN_KIND',
  [OperationSupport.StateSchemaTooNew]: 'STATE_SCHEMA_TOO_NEW',
}

export const NETWORKS: Record<Network, string> = {
  [Network.Bitcoin]: 'BITCOIN',
  [Network.Testnet]: 'TESTNET',
  [Network.Testnet4]: 'TESTNET4',
  [Network.Signet]: 'SIGNET',
  [Network.Regtest]: 'REGTEST',
}

export const DIRECTIONS: Record<Direction, string> = {
  [Direction.Incoming]: 'INCOMING',
  [Direction.Outgoing]: 'OUTGOING',
}

export const ACTIVITY_STATUSES: Record<ActivityStatus, string> = {
  [ActivityStatus.Pending]: 'PENDING',
  [ActivityStatus.Success]: 'SUCCESS',
  [ActivityStatus.Failed]: 'FAILED',
  [ActivityStatus.Refunded]: 'REFUNDED',
  [ActivityStatus.Canceled]: 'CANCELED',
  [ActivityStatus.Unknown]: 'UNKNOWN',
}

const ECASH_SEND_STATES: Record<EcashSendState, string> = {
  [EcashSendState.Created]: 'CREATED',
  [EcashSendState.CancelRequested]: 'CANCEL_REQUESTED',
  [EcashSendState.Canceled]: 'CANCELED',
  [EcashSendState.Redeemed]: 'REDEEMED',
}

/** Any thrown error as the protocol's `{code, reason}`. */
export function encodeError(error: unknown): Json {
  if (error instanceof HarnessError) {
    return { code: error.code, reason: error.message }
  }
  if (error instanceof SdkError) {
    return { code: ERROR_CODES[error.code], reason: error.reason }
  }
  const e = error instanceof Error ? error : new Error(String(error))
  return { code: 'HARNESS_INTERNAL', reason: `${e.name}: ${e.message}` }
}

function unexpected(value: never): never {
  throw new HarnessError(
    'HARNESS_INTERNAL',
    `unencoded variant ${JSON.stringify(value)}`,
  )
}

/** A tagged value as the worker hands it over: the tag and its fields. */
type Tagged<T extends string, F = undefined> = F extends undefined
  ? { tag: T }
  : { tag: T; inner: F }

export type Route =
  | Tagged<LightningRoute_Tags.Internal>
  | Tagged<LightningRoute_Tags.Gateway, { gatewayId: string }>

export function route(value: Route): Json {
  switch (value.tag) {
    case LightningRoute_Tags.Internal:
      return { type: 'INTERNAL' }
    case LightningRoute_Tags.Gateway:
      return { type: 'GATEWAY', gatewayId: value.inner.gatewayId }
    default:
      return unexpected(value)
  }
}

export function ecashSendState(state: EcashSendState): Json {
  return { type: ECASH_SEND_STATES[state] }
}

type Reason = { reason: string }

export function ecashReceiveState(
  state:
    | Tagged<EcashReceiveState_Tags.Created>
    | Tagged<EcashReceiveState_Tags.Issuing>
    | Tagged<EcashReceiveState_Tags.Done>
    | Tagged<EcashReceiveState_Tags.Failed, Reason>,
): Json {
  switch (state.tag) {
    case EcashReceiveState_Tags.Created:
      return { type: 'CREATED' }
    case EcashReceiveState_Tags.Issuing:
      return { type: 'ISSUING' }
    case EcashReceiveState_Tags.Done:
      return { type: 'DONE' }
    case EcashReceiveState_Tags.Failed:
      return { type: 'FAILED', reason: state.inner.reason }
    default:
      return unexpected(state)
  }
}

export function lnSendState(
  state:
    | Tagged<LnSendState_Tags.Created>
    | Tagged<LnSendState_Tags.Funded>
    | Tagged<
        LnSendState_Tags.Success,
        { preimage: string; fee: bigint; route: Route }
      >
    | Tagged<LnSendState_Tags.Refunded>
    | Tagged<LnSendState_Tags.Failed, Reason>,
): Json {
  switch (state.tag) {
    case LnSendState_Tags.Created:
      return { type: 'CREATED' }
    case LnSendState_Tags.Funded:
      return { type: 'FUNDED' }
    case LnSendState_Tags.Success:
      return {
        type: 'SUCCESS',
        preimage: state.inner.preimage,
        feeMsats: num(state.inner.fee),
        route: route(state.inner.route),
      }
    case LnSendState_Tags.Refunded:
      return { type: 'REFUNDED' }
    case LnSendState_Tags.Failed:
      return { type: 'FAILED', reason: state.inner.reason }
    default:
      return unexpected(state)
  }
}

export function lnReceiveState(
  state:
    | Tagged<LnReceiveState_Tags.Created>
    | Tagged<LnReceiveState_Tags.WaitingForPayment>
    | Tagged<LnReceiveState_Tags.Funded>
    | Tagged<LnReceiveState_Tags.Claimed>
    | Tagged<LnReceiveState_Tags.Canceled, Reason>
    | Tagged<LnReceiveState_Tags.Expired>
    | Tagged<LnReceiveState_Tags.Failed>,
): Json {
  switch (state.tag) {
    case LnReceiveState_Tags.Created:
      return { type: 'CREATED' }
    case LnReceiveState_Tags.WaitingForPayment:
      return { type: 'WAITING_FOR_PAYMENT' }
    case LnReceiveState_Tags.Funded:
      return { type: 'FUNDED' }
    case LnReceiveState_Tags.Claimed:
      return { type: 'CLAIMED' }
    case LnReceiveState_Tags.Canceled:
      return { type: 'CANCELED', reason: state.inner.reason }
    case LnReceiveState_Tags.Expired:
      return { type: 'EXPIRED' }
    case LnReceiveState_Tags.Failed:
      return { type: 'FAILED' }
    default:
      return unexpected(state)
  }
}

export function onchainSendState(
  state:
    | Tagged<OnchainSendState_Tags.Created>
    | Tagged<OnchainSendState_Tags.Succeeded, { txid: string }>
    | Tagged<OnchainSendState_Tags.Refunded, Reason>
    | Tagged<OnchainSendState_Tags.Failed, Reason>,
): Json {
  switch (state.tag) {
    case OnchainSendState_Tags.Created:
      return { type: 'CREATED' }
    case OnchainSendState_Tags.Succeeded:
      return { type: 'SUCCEEDED', txid: state.inner.txid }
    case OnchainSendState_Tags.Refunded:
      return { type: 'REFUNDED', reason: state.inner.reason }
    case OnchainSendState_Tags.Failed:
      return { type: 'FAILED', reason: state.inner.reason }
    default:
      return unexpected(state)
  }
}

export function onchainReceiveState(
  state:
    | Tagged<
        OnchainReceiveState_Tags.Confirmed,
        { txid: string; grossDeposited: bigint }
      >
    | Tagged<
        OnchainReceiveState_Tags.Claimed,
        { txid: string; grossDeposited: bigint; netCredit: bigint }
      >
    | Tagged<OnchainReceiveState_Tags.Failed, Reason>,
): Json {
  switch (state.tag) {
    case OnchainReceiveState_Tags.Confirmed:
      return {
        type: 'CONFIRMED',
        txid: state.inner.txid,
        grossDepositedSats: num(state.inner.grossDeposited),
      }
    case OnchainReceiveState_Tags.Claimed:
      return {
        type: 'CLAIMED',
        txid: state.inner.txid,
        grossDepositedSats: num(state.inner.grossDeposited),
        netCreditMsats: num(state.inner.netCredit),
      }
    case OnchainReceiveState_Tags.Failed:
      return { type: 'FAILED', reason: state.inner.reason }
    default:
      return unexpected(state)
  }
}

export function recoveryState(
  state:
    | Tagged<
        RecoveryState_Tags.Running,
        { progress?: { complete: number; total: number } }
      >
    | Tagged<RecoveryState_Tags.Done>
    | Tagged<RecoveryState_Tags.Failed, Reason>,
): Json {
  switch (state.tag) {
    case RecoveryState_Tags.Running: {
      const p = state.inner.progress
      return {
        type: 'RUNNING',
        progress: p ? { complete: p.complete, total: p.total } : null,
      }
    }
    case RecoveryState_Tags.Done:
      return { type: 'DONE' }
    case RecoveryState_Tags.Failed:
      return { type: 'FAILED', reason: state.inner.reason }
    default:
      return unexpected(state)
  }
}

// ── Params ─────────────────────────────────────────────────────────────────

export class Params {
  constructor(private readonly values: Json) {}

  private present(key: string): boolean {
    return this.values[key] !== undefined && this.values[key] !== null
  }

  str(key: string): string {
    if (!(key in this.values)) throw badParams(`missing "${key}"`)
    const value = this.values[key]
    if (typeof value !== 'string') throw badParams(`"${key}" must be a string`)
    return value
  }

  optStr(key: string): string | undefined {
    return this.present(key) ? this.str(key) : undefined
  }

  /** A non-negative integer: an amount, a limit or a timeout. */
  uint(key: string): bigint {
    if (!(key in this.values)) throw badParams(`missing "${key}"`)
    const value = this.values[key]
    if (
      typeof value !== 'number' ||
      !Number.isSafeInteger(value) ||
      value < 0
    ) {
      throw badParams(`"${key}" must be a non-negative integer`)
    }
    return BigInt(value)
  }

  optUint(key: string): bigint | undefined {
    return this.present(key) ? this.uint(key) : undefined
  }

  optStrList(key: string): string[] | undefined {
    if (!this.present(key)) return undefined
    const value = this.values[key]
    if (!Array.isArray(value) || !value.every((v) => typeof v === 'string')) {
      throw badParams(`"${key}" must be an array of strings`)
    }
    return value
  }
}
