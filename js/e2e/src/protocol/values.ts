// The SDK values a harness encodes, as PROTOCOL.md names them: enums as the
// SCREAMING_SNAKE of the Rust variant, every state as `{type, ...fields}`.

import { Decoded, hex, nullable, obj, oneOf, str, tagged, uint } from './decode'

/** rust/fedimint-sdk/src/error.rs `ErrorCode`, in declaration order. */
export const ERROR_CODES = [
  'INVALID_INPUT',
  'ALREADY_JOINED',
  'UNSUPPORTED_FEDERATION',
  'FEDERATION_UNREACHABLE',
  'INSUFFICIENT_BALANCE',
  'BALANCE_NOT_EMPTY',
  'PENDING_OPERATIONS',
  'GATEWAY_UNAVAILABLE',
  'RECOVERING',
  'NOT_SUPPORTED',
  'UNSUPPORTED_OPERATION',
  'SEED_MISMATCH',
  'STORAGE_IN_USE',
  'STORAGE_ORPHANED',
  'QUOTE_EXPIRED',
  'QUOTE_CHANGED',
  'AMOUNTLESS_INVOICE',
  'NETWORK_MISMATCH',
  'FEDERATION_CLOSED',
  'TIMEOUT',
  'ENTROPY',
  'STORAGE',
  'INTERNAL',
] as const
export type ErrorCode = (typeof ERROR_CODES)[number]

/** Failures of the harness itself rather than of an SDK call. */
export const HARNESS_CODES = [
  'HARNESS_BAD_REQUEST',
  'HARNESS_UNKNOWN_METHOD',
  'HARNESS_BAD_PARAMS',
  'HARNESS_STATE',
  'HARNESS_UNKNOWN_REF',
  'HARNESS_TIMEOUT',
  'HARNESS_BUSY',
  'HARNESS_INTERNAL',
] as const
export type HarnessCode = (typeof HARNESS_CODES)[number]

export const OPERATION_KINDS = [
  'ECASH_SEND',
  'ECASH_RECEIVE',
  'LN_SEND',
  'LN_RECEIVE',
  'ONCHAIN_SEND',
  'ONCHAIN_RECEIVE',
  'RECOVERY',
  'UNKNOWN',
] as const
export type OperationKind = (typeof OPERATION_KINDS)[number]

export const NETWORKS = [
  'BITCOIN',
  'TESTNET',
  'TESTNET4',
  'SIGNET',
  'REGTEST',
] as const
export const DIRECTIONS = ['INCOMING', 'OUTGOING'] as const
export const ACTIVITY_STATUSES = [
  'PENDING',
  'SUCCESS',
  'FAILED',
  'REFUNDED',
  'CANCELED',
  'UNKNOWN',
] as const
export const OPERATION_SUPPORT = [
  'OBSERVABLE',
  'UNKNOWN_KIND',
  'STATE_SCHEMA_TOO_NEW',
] as const

export const errorValue = obj({
  code: oneOf([...ERROR_CODES, ...HARNESS_CODES]),
  reason: str,
})
export type ErrorValue = Decoded<typeof errorValue>

export const route = tagged({
  INTERNAL: {},
  GATEWAY: { gatewayId: str },
})
export type Route = Decoded<typeof route>

const reason = { reason: str }
const txid = hex(32)

/** The states each operation kind can report, keyed by kind. */
export const STATES = {
  ECASH_SEND: tagged({
    CREATED: {},
    CANCEL_REQUESTED: {},
    CANCELED: {},
    REDEEMED: {},
  }),
  ECASH_RECEIVE: tagged({
    CREATED: {},
    ISSUING: {},
    DONE: {},
    FAILED: reason,
  }),
  LN_SEND: tagged({
    CREATED: {},
    FUNDED: {},
    SUCCESS: { preimage: hex(32), feeMsats: uint, route },
    REFUNDED: {},
    FAILED: reason,
  }),
  LN_RECEIVE: tagged({
    CREATED: {},
    WAITING_FOR_PAYMENT: {},
    FUNDED: {},
    CLAIMED: {},
    CANCELED: reason,
    EXPIRED: {},
    FAILED: {},
  }),
  ONCHAIN_SEND: tagged({
    CREATED: {},
    SUCCEEDED: { txid },
    REFUNDED: reason,
    FAILED: reason,
  }),
  ONCHAIN_RECEIVE: tagged({
    CONFIRMED: { txid, grossDepositedSats: uint },
    CLAIMED: { txid, grossDepositedSats: uint, netCreditMsats: uint },
    FAILED: reason,
  }),
  RECOVERY: tagged({
    RUNNING: {
      progress: nullable(obj({ complete: uint, total: uint })),
    },
    DONE: {},
    FAILED: reason,
  }),
} as const

export type ObservableKind = keyof typeof STATES
export type StateOf<K extends ObservableKind> = Decoded<(typeof STATES)[K]>
/** Any operation's state. Narrow it with `kind` and `type`. */
export type OperationState = StateOf<ObservableKind>

/** Decodes a state for an operation of `kind`; kinds the SDK can't observe
 * (UNKNOWN) have no state to decode. */
export function decodeState(
  kind: OperationKind,
  value: unknown,
  path: string,
): OperationState {
  if (kind === 'UNKNOWN') {
    throw new Error(`${path}: an UNKNOWN operation has no state`)
  }
  return STATES[kind](value, path)
}
