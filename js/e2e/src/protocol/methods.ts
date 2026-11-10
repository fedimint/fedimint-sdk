// Every method a harness serves: its params and a decoder for its result.
// PROTOCOL.md is the prose of this file; keep the two in step.

import {
  arr,
  bool,
  Decoded,
  Decoder,
  hex,
  literal,
  nullable,
  obj,
  oneOf,
  record,
  str,
  uint,
} from './decode'
import {
  ACTIVITY_STATUSES,
  decodeState,
  DIRECTIONS,
  errorValue,
  NETWORKS,
  OPERATION_KINDS,
  OPERATION_SUPPORT,
  OperationKind,
  OperationState,
  route,
} from './values'

type Empty = Record<string, never>

export interface Params {
  'harness.describe': Empty
  'harness.state': Empty
  'harness.echo': { payload: string }
  'sdk.open': { mnemonic?: string[] | null }
  'sdk.mnemonic': Empty
  'invite.parse': { inviteCode: string }
  'sdk.preview': { inviteCode: string }
  'sdk.join': { inviteCode: string }
  'federation.info': Empty
  'federation.balance': Empty
  'federation.observedBalance': Empty
  'lightning.receive': { amountMsats: number; description: string }
  'lightning.quote': { invoice: string }
  'lightning.send': { quoteId: string }
  'ecash.quote': { amountMsats: number }
  'ecash.send': { quoteId: string }
  'ecash.receive': { notes: string } | { notesRef: string }
  'onchain.receive': Empty
  'onchain.deposits': Empty
  'onchain.quote': { address: string; amountSats: number }
  'onchain.send': { quoteId: string }
  'operation.lookup': { operationId: string }
  'operation.observed': { operationId: string }
  'operation.awaitFinal': { operationId: string; timeoutMs?: number }
  'activity.list': { limit?: number; cursor?: string | null }
}

const id32 = hex(32)
const network = oneOf(NETWORKS)
const kind = oneOf(OPERATION_KINDS)
const operationRef = obj({ operationId: id32 })

/** `{found:false}`, or the operation's kind, support and current state (null
 * when this build has no typed handle for it). */
export interface LookupFound {
  found: true
  kind: OperationKind
  support: (typeof OPERATION_SUPPORT)[number]
  state: OperationState | null
}
const lookup: Decoder<{ found: false } | LookupFound> = (v, p) => {
  if (!obj({ found: bool })(v, p).found) return { found: false }
  const head = obj({
    kind,
    support: oneOf(OPERATION_SUPPORT),
    state: (x: unknown) => x,
  })(v, p)
  const state =
    head.state === null
      ? null
      : decodeState(head.kind, head.state, `${p}.state`)
  return { found: true, kind: head.kind, support: head.support, state }
}

const observed = (v: unknown, p: string) => {
  const head = obj({
    kind,
    states: arr((x: unknown) => x),
    final: bool,
    error: nullable(errorValue),
  })(v, p)
  const states = head.states.map((s, i) =>
    decodeState(head.kind, s, `${p}.states[${i}]`),
  )
  return { ...head, states }
}

const finalState = (v: unknown, p: string) => {
  const head = obj({ kind, state: (x: unknown) => x })(v, p)
  return {
    kind: head.kind,
    state: decodeState(head.kind, head.state, `${p}.state`),
  }
}

const activityItem = obj({
  operationId: id32,
  kind,
  timeMs: uint,
  amountMsats: nullable(uint),
  feeMsats: nullable(uint),
  direction: nullable(oneOf(DIRECTIONS)),
  status: oneOf(ACTIVITY_STATUSES),
  isFinal: bool,
})

export const RESULTS = {
  'harness.describe': obj({
    protocolVersion: literal(1),
    platform: str,
    sdkVersion: nullable(str),
    methods: arr(str),
    features: arr(str),
  }),
  'harness.state': obj({
    sdkOpen: bool,
    federationId: nullable(id32),
    operations: uint,
    quotes: uint,
  }),
  'harness.echo': obj({ payload: str }),
  'sdk.open': obj({
    federationIds: arr(id32),
    attachedFederationId: nullable(id32),
  }),
  'sdk.mnemonic': obj({ words: arr(str) }),
  'invite.parse': obj({ federationId: id32 }),
  'sdk.preview': obj({
    id: id32,
    name: nullable(str),
    network,
    guardians: uint,
    modules: arr(str),
    meta: record(str),
  }),
  'sdk.join': obj({ federationId: id32 }),
  'federation.info': obj({
    id: id32,
    name: nullable(str),
    network,
    capabilities: obj({ ecash: bool, lightning: bool, onchain: bool }),
  }),
  'federation.balance': obj({ amountMsats: uint }),
  'federation.observedBalance': obj({
    amountMsats: nullable(uint),
    updates: uint,
    error: nullable(errorValue),
  }),
  'lightning.receive': obj({ invoice: str, operationId: id32 }),
  'lightning.quote': obj({
    quoteId: str,
    invoiceAmountMsats: uint,
    feeMsats: uint,
    totalMsats: uint,
    route,
    expiresAtMs: uint,
  }),
  'lightning.send': operationRef,
  'ecash.quote': obj({
    quoteId: str,
    requestedAmountMsats: uint,
    notesValueMsats: uint,
    feeMsats: uint,
    totalMsats: uint,
    expiresAtMs: uint,
  }),
  'ecash.send': obj({
    operationId: id32,
    notesRef: str,
    notesValueMsats: uint,
  }),
  'ecash.receive': obj({ operationId: id32, notesValueMsats: uint }),
  'onchain.receive': obj({ address: str }),
  'onchain.deposits': obj({
    operationIds: arr(id32),
    error: nullable(errorValue),
  }),
  'onchain.quote': obj({
    quoteId: str,
    amountSats: uint,
    feeMsats: uint,
    totalMsats: uint,
    expiresAtMs: uint,
  }),
  'onchain.send': operationRef,
  'operation.lookup': lookup,
  'operation.observed': observed,
  'operation.awaitFinal': finalState,
  'activity.list': obj({ items: arr(activityItem), next: nullable(str) }),
} satisfies { [M in keyof Params]: Decoder<unknown> }

export type Method = keyof Params
export type ParamsOf<M extends Method> = Params[M]
export type ResultOf<M extends Method> = Decoded<(typeof RESULTS)[M]>

/** Every method, in PROTOCOL.md's order. */
export const METHODS = Object.keys(RESULTS) as Method[]
