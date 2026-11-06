import type { OpenOptions, SdkSession } from '@fedimint/web-sdk'

import {
  ACTIVITY_STATUSES,
  DIRECTIONS,
  ecashReceiveState,
  ecashSendState,
  encodeError,
  HarnessError,
  Json,
  lnReceiveState,
  lnSendState,
  NETWORKS,
  num,
  onchainReceiveState,
  onchainSendState,
  OPERATION_KINDS,
  OPERATION_SUPPORT,
  orNull,
  Params,
  recoveryState,
  route,
} from './encode'

/** js/e2e/PROTOCOL.md's version; a harness and the suite must agree on it. */
export const PROTOCOL_VERSION = 1

// The worker proxies hand out handles whose methods are all async; the
// harness reads them loosely and leaves the typing to the encoders.
type Handle = any

/** One typed operation handle, reduced to what the protocol needs. */
interface TypedOp {
  id: string
  kind: string
  state(): Promise<Json>
  awaitFinal(): Promise<Json>
  /** Delivers each state `updates()` yields until the subscription ends. */
  follow(emit: (state: Json) => void): Promise<void>
}

async function typed(
  op: Handle,
  kind: string,
  encode: (state: Handle) => Json,
): Promise<TypedOp> {
  return {
    id: await op.id(),
    kind,
    state: async () => encode(await op.state()),
    awaitFinal: async () => encode(await op.awaitFinal()),
    follow: async (emit) => {
      const updates = await op.updates()
      for (;;) {
        const state = await updates.next()
        if (state === undefined || state === null) return
        emit(encode(state))
      }
    },
  }
}

const TYPED: [string, string, (state: Handle) => Json][] = [
  ['asEcashSend', 'ECASH_SEND', ecashSendState],
  ['asEcashReceive', 'ECASH_RECEIVE', ecashReceiveState],
  ['asLnSend', 'LN_SEND', lnSendState],
  ['asLnReceive', 'LN_RECEIVE', lnReceiveState],
  ['asOnchainSend', 'ONCHAIN_SEND', onchainSendState],
  ['asOnchainReceive', 'ONCHAIN_RECEIVE', onchainReceiveState],
  ['asRecovery', 'RECOVERY', recoveryState],
]

/** The typed handle behind a type-erased one, or null when this build has none. */
async function typedOf(op: Handle): Promise<TypedOp | null> {
  for (const [downcast, kind, encode] of TYPED) {
    const handle = await op[downcast]()
    if (handle) return typed(handle, kind, encode)
  }
  return null
}

/** What one operation's watcher has seen. */
class OpRecord {
  states: Json[] = []
  final = false
  error: Json | null = null
  constructor(readonly op: TypedOp) {}

  json(): Json {
    return {
      kind: this.op.kind,
      states: this.states,
      final: this.final,
      error: this.error,
    }
  }
}

type Method = (p: Params) => Promise<Json>

/**
 * The harness: the console's request handling and every protocol method,
 * over the web SDK. One call runs at a time; watchers keep appending to
 * their records alongside. `open` is injected, so another platform built on
 * the same JavaScript API (React Native) can reuse this unchanged.
 */
export class Harness {
  response = JSON.stringify({ v: PROTOCOL_VERSION, ready: true })
  onResponse: (text: string) => void = () => {}

  private session: SdkSession | null = null
  private federation: Handle = null
  private quotes = new Map<string, { kind: string; quote: Handle }>()
  private notes = new Map<string, Handle>()
  private operations = new Map<string, OpRecord>()
  private balance: {
    amount: bigint | null
    updates: number
    error: Json | null
  } = { amount: null, updates: 0, error: null }
  private depositsOpen = false
  private depositIds: string[] = []
  private depositsError: Json | null = null
  // Bumped on every attach, so a subscription loop of an earlier federation
  // stops recording into this one's state.
  private generation = 0
  private refs = 0
  private inFlight = false
  private lastId: string | null = null

  constructor(
    private readonly open: (options: OpenOptions) => Promise<SdkSession>,
    private readonly storage: string,
    private readonly platform = 'web',
  ) {}

  // ── The console ──────────────────────────────────────────────────────

  /** Takes the request text from a click on `run`. */
  submit(raw: string): void {
    let request: { id: string; method: string; params: Json }
    try {
      request = parse(raw)
    } catch (error) {
      this.publish({
        v: PROTOCOL_VERSION,
        id: null,
        ok: false,
        error: encodeError(error),
        received: raw.slice(0, 200),
      })
      return
    }
    // A repeated id is a retried click: leave its answer showing.
    if (request.id === this.lastId) return
    if (this.inFlight) {
      this.publish({
        v: PROTOCOL_VERSION,
        id: request.id,
        ok: false,
        error: {
          code: 'HARNESS_BUSY',
          reason: 'another call is still running',
        },
        ms: 0,
      })
      return
    }
    this.inFlight = true
    this.lastId = request.id
    this.publish({ v: PROTOCOL_VERSION, id: request.id, pending: true })

    const started = performance.now()
    this.call(request.method, new Params(request.params)).then(
      (result) => this.finish({ id: request.id, ok: true, result }, started),
      (error) =>
        this.finish(
          { id: request.id, ok: false, error: encodeError(error) },
          started,
        ),
    )
  }

  private finish(answer: Json, started: number): void {
    this.inFlight = false
    this.publish({
      v: PROTOCOL_VERSION,
      ...answer,
      ms: Math.round(performance.now() - started),
    })
  }

  private publish(envelope: Json): void {
    try {
      this.response = JSON.stringify(envelope)
    } catch (error) {
      this.response = JSON.stringify({
        v: PROTOCOL_VERSION,
        id: envelope.id ?? null,
        ok: false,
        error: encodeError(error),
      })
    }
    this.onResponse(this.response)
  }

  private async call(method: string, params: Params): Promise<Json> {
    const handler = this.methods[method]
    if (!handler) {
      throw new HarnessError('HARNESS_UNKNOWN_METHOD', `no method "${method}"`)
    }
    return handler(params)
  }

  // ── Methods, in PROTOCOL.md's order ──────────────────────────────────

  private readonly methods: Record<string, Method> = {
    'harness.describe': async () => ({
      protocolVersion: PROTOCOL_VERSION,
      platform: this.platform,
      sdkVersion: null,
      methods: Object.keys(this.methods),
      features: [],
    }),
    'harness.state': async () => ({
      sdkOpen: this.session !== null,
      federationId: this.federation ? await this.federation.id() : null,
      operations: this.operations.size,
      quotes: this.quotes.size,
    }),
    'harness.echo': async (p) => ({ payload: p.str('payload') }),

    'sdk.open': async (p) => this.openSdk(p.optStrList('mnemonic')),
    'sdk.mnemonic': async () => {
      const mnemonic = await this.sdk().sdk.exportMnemonic()
      return { words: await mnemonic.words() }
    },
    'invite.parse': async (p) => {
      const code = p.str('inviteCode')
      const invite = await this.sdk().InviteCode.parse(code)
      return { federationId: await invite.federationId() }
    },
    'sdk.preview': async (p) => {
      const code = p.str('inviteCode')
      const session = this.sdk()
      const preview = await session.sdk.preview(
        await session.InviteCode.parse(code),
      )
      return {
        id: preview.id,
        name: orNull(preview.name),
        network: NETWORKS[preview.network],
        guardians: preview.guardians,
        modules: preview.modules,
        meta: Object.fromEntries(preview.meta),
      }
    },
    'sdk.join': async (p) => {
      const code = p.str('inviteCode')
      const session = this.sdk()
      const joined = await session.sdk.join(
        await session.InviteCode.parse(code),
      )
      await this.attach(joined)
      return { federationId: await joined.id() }
    },

    'federation.info': async () => {
      const f = this.requireFederation()
      const caps = await f.capabilities()
      return {
        id: await f.id(),
        name: orNull(await f.name()),
        network: NETWORKS[(await f.network()) as keyof typeof NETWORKS],
        capabilities: {
          ecash: caps.ecash,
          lightning: caps.lightning,
          onchain: caps.onchain,
        },
      }
    },
    'federation.balance': async () => ({
      amountMsats: num(await this.requireFederation().balance()),
    }),
    'federation.observedBalance': async () => {
      this.requireFederation()
      return {
        amountMsats:
          this.balance.amount === null ? null : num(this.balance.amount),
        updates: this.balance.updates,
        error: this.balance.error,
      }
    },

    'lightning.receive': async (p) => {
      const amount = p.uint('amountMsats')
      const description = p.str('description')
      const receive = await (
        await this.module('lightning')
      ).receive(amount, description)
      const op = this.track(
        await typed(receive.operation, 'LN_RECEIVE', lnReceiveState),
      )
      return { invoice: receive.invoice, operationId: op.op.id }
    },
    'lightning.quote': async (p) => {
      const invoice = p.str('invoice')
      const quote = await (await this.module('lightning')).quote(invoice)
      return {
        quoteId: this.hold('lightning', quote),
        invoiceAmountMsats: num(await quote.invoiceAmount()),
        feeMsats: num(await quote.fee()),
        totalMsats: num(await quote.total()),
        route: route(await quote.route()),
        expiresAtMs: num(await quote.expiresAt()),
      }
    },
    'lightning.send': async (p) => {
      const quote = this.held(p.str('quoteId'), 'lightning')
      const op = await (await this.module('lightning')).send(quote)
      const record = this.track(await typed(op, 'LN_SEND', lnSendState))
      return { operationId: record.op.id }
    },

    'ecash.quote': async (p) => {
      const amount = p.uint('amountMsats')
      const quote = await (await this.module('ecash')).quote(amount)
      return {
        quoteId: this.hold('ecash', quote),
        requestedAmountMsats: num(await quote.requestedAmount()),
        notesValueMsats: num(await quote.notesValue()),
        feeMsats: num(await quote.fee()),
        totalMsats: num(await quote.total()),
        expiresAtMs: num(await quote.expiresAt()),
      }
    },
    'ecash.send': async (p) => {
      const quote = this.held(p.str('quoteId'), 'ecash')
      const sent = await (await this.module('ecash')).send(quote)
      const ref = `n${++this.refs}`
      this.notes.set(ref, sent.notes)
      const record = this.track(
        await typed(sent.operation, 'ECASH_SEND', ecashSendState),
      )
      return {
        operationId: record.op.id,
        notesRef: ref,
        notesValueMsats: num(await sent.notes.value()),
      }
    },
    'ecash.receive': async (p) => {
      const text = p.optStr('notes')
      const ref = p.optStr('notesRef')
      if ((text === undefined) === (ref === undefined)) {
        throw new HarnessError(
          'HARNESS_BAD_PARAMS',
          'pass exactly one of "notes" and "notesRef"',
        )
      }
      const ecash = await this.module('ecash')
      // A held note goes back out through display() and in through parse(),
      // so the string round trip is exercised either way.
      const held = ref === undefined ? undefined : this.notes.get(ref)
      if (ref !== undefined && !held) {
        throw new HarnessError('HARNESS_UNKNOWN_REF', `no notes "${ref}"`)
      }
      const source = text ?? ((await held.display()) as string)
      const notes = await this.sdk().Notes.parse(source)
      const value = await notes.value()
      const op = await ecash.receive(notes)
      const record = this.track(
        await typed(op, 'ECASH_RECEIVE', ecashReceiveState),
      )
      return { operationId: record.op.id, notesValueMsats: num(value) }
    },

    'onchain.receive': async () => {
      const onchain = await this.module('onchain')
      // deposits() first: it reports only deposits found after it is
      // opened, so opening it after showing the address could miss one.
      if (!this.depositsOpen) {
        this.followDeposits(await onchain.deposits())
        this.depositsOpen = true
      }
      return { address: await onchain.receive() }
    },
    'onchain.deposits': async () => {
      this.requireFederation()
      return { operationIds: this.depositIds, error: this.depositsError }
    },
    'onchain.quote': async (p) => {
      const address = p.str('address')
      const amount = p.uint('amountSats')
      const quote = await (await this.module('onchain')).quote(address, amount)
      return {
        quoteId: this.hold('onchain', quote),
        amountSats: num(await quote.amount()),
        feeMsats: num(await quote.fee()),
        totalMsats: num(await quote.total()),
        expiresAtMs: num(await quote.expiresAt()),
      }
    },
    'onchain.send': async (p) => {
      const quote = this.held(p.str('quoteId'), 'onchain')
      const op = await (await this.module('onchain')).send(quote)
      const record = this.track(
        await typed(op, 'ONCHAIN_SEND', onchainSendState),
      )
      return { operationId: record.op.id }
    },

    'operation.lookup': async (p) => {
      const id = p.str('operationId')
      const op = await this.requireFederation().operation(id)
      if (!op) return { found: false }
      const typedOp = await typedOf(op)
      const state = typedOp ? await typedOp.state() : null
      if (typedOp) this.track(typedOp)
      return {
        found: true,
        kind: OPERATION_KINDS[
          (await op.kind()) as keyof typeof OPERATION_KINDS
        ],
        support:
          OPERATION_SUPPORT[
            (await op.support()) as keyof typeof OPERATION_SUPPORT
          ],
        state,
      }
    },
    'operation.observed': async (p) => this.record(p.str('operationId')).json(),
    'operation.awaitFinal': async (p) => {
      const record = this.record(p.str('operationId'))
      const timeoutMs = Number(p.optUint('timeoutMs') ?? 120_000n)
      let timer: ReturnType<typeof setTimeout> | undefined
      const state = await Promise.race([
        record.op.awaitFinal(),
        new Promise<null>((resolve) => {
          timer = setTimeout(() => resolve(null), timeoutMs)
        }),
      ]).finally(() => clearTimeout(timer))
      if (state === null) {
        throw new HarnessError(
          'HARNESS_TIMEOUT',
          `not final within ${timeoutMs}ms`,
        )
      }
      return { kind: record.op.kind, state }
    },

    'activity.list': async (p) => {
      const limit = Number(p.optUint('limit') ?? 20n)
      const page = await this.requireFederation().activity(
        p.optStr('cursor'),
        Math.min(limit, 65_535),
      )
      return {
        items: page.items.map((item: Handle) => ({
          operationId: item.operationId,
          kind: OPERATION_KINDS[item.kind as keyof typeof OPERATION_KINDS],
          timeMs: num(item.time),
          amountMsats: item.amount === undefined ? null : num(item.amount),
          feeMsats: item.fee === undefined ? null : num(item.fee),
          direction:
            item.direction === undefined
              ? null
              : DIRECTIONS[item.direction as keyof typeof DIRECTIONS],
          status:
            ACTIVITY_STATUSES[item.status as keyof typeof ACTIVITY_STATUSES],
          isFinal: item.isFinal,
        })),
        next: orNull(page.next),
      }
    },
  }

  // ── SDK state ────────────────────────────────────────────────────────

  /**
   * `openSdk` over the harness's storage, then the first federation it has
   * joined, if any, so a joined federation survives a reload. With words,
   * restores that seed instead.
   */
  private async openSdk(words: string[] | undefined): Promise<Json> {
    if (this.session) {
      throw new HarnessError('HARNESS_STATE', 'the SDK is already open')
    }
    this.session = await this.open({ storage: this.storage, mnemonic: words })
    const federations: Handle[] = await this.session.sdk.federations()
    const ids: string[] = await Promise.all(federations.map((f) => f.id()))
    if (federations[0]) await this.attach(federations[0])
    return { federationIds: ids, attachedFederationId: ids[0] ?? null }
  }

  /** Makes `joined` the current federation and subscribes to its balance.
   * Quotes, notes, operations and deposits belong to the previous one. */
  private async attach(joined: Handle): Promise<void> {
    if (
      this.federation &&
      (await this.federation.id()) === (await joined.id())
    ) {
      return
    }
    const generation = ++this.generation
    this.federation = joined
    this.quotes.clear()
    this.notes.clear()
    this.operations.clear()
    this.depositsOpen = false
    this.depositIds = []
    this.depositsError = null
    const balance = {
      amount: null as bigint | null,
      updates: 0,
      error: null as Json | null,
    }
    this.balance = balance
    void (async () => {
      try {
        balance.amount = await joined.balance()
        const updates = await joined.balanceUpdates()
        while (generation === this.generation) {
          balance.amount = await updates.next()
          balance.updates++
        }
      } catch (error) {
        if (generation === this.generation) balance.error = encodeError(error)
      }
    })()
  }

  /** Follows `deposits` for the federation's lifetime: each deposit the
   * wallet finds becomes an operation, tracked like any other. */
  private followDeposits(deposits: Handle): void {
    const generation = this.generation
    void (async () => {
      try {
        while (generation === this.generation) {
          const op = await deposits.next()
          if (generation !== this.generation) return
          const record = this.track(
            await typed(op, 'ONCHAIN_RECEIVE', onchainReceiveState),
          )
          this.depositIds.push(record.op.id)
        }
      } catch (error) {
        if (generation === this.generation) {
          this.depositsError = encodeError(error)
        }
      }
    })()
  }

  /** Starts one watcher for an operation, or returns the one it has. */
  private track(op: TypedOp): OpRecord {
    const existing = this.operations.get(op.id)
    if (existing) return existing
    const record = new OpRecord(op)
    this.operations.set(op.id, record)
    op.follow((state) => record.states.push(state)).then(
      () => {
        record.final = true
      },
      (error) => {
        record.error = encodeError(error)
      },
    )
    return record
  }

  private sdk(): SdkSession {
    if (!this.session) {
      throw new HarnessError(
        'HARNESS_STATE',
        'the SDK is not open; call sdk.open',
      )
    }
    return this.session
  }

  private requireFederation(): Handle {
    if (!this.federation) {
      throw new HarnessError('HARNESS_STATE', 'no federation is attached')
    }
    return this.federation
  }

  private async module(
    name: 'lightning' | 'ecash' | 'onchain',
  ): Promise<Handle> {
    const module = await this.requireFederation()[name]()
    if (!module) {
      const label = { lightning: 'lightning', ecash: 'mint', onchain: 'wallet' }
      throw new HarnessError('HARNESS_STATE', `no ${label[name]} module`)
    }
    return module
  }

  private hold(kind: string, quote: Handle): string {
    const id = `q${++this.refs}`
    this.quotes.set(id, { kind, quote })
    return id
  }

  private held(id: string, kind: string): Handle {
    const entry = this.quotes.get(id)
    if (!entry) {
      throw new HarnessError('HARNESS_UNKNOWN_REF', `no quote "${id}"`)
    }
    if (entry.kind !== kind) {
      throw new HarnessError(
        'HARNESS_BAD_PARAMS',
        `quote "${id}" is a ${entry.kind} quote`,
      )
    }
    return entry.quote
  }

  private record(id: string): OpRecord {
    const record = this.operations.get(id)
    if (!record) {
      throw new HarnessError(
        'HARNESS_UNKNOWN_REF',
        `no operation "${id}" is tracked`,
      )
    }
    return record
  }
}

function parse(raw: string): { id: string; method: string; params: Json } {
  const bad = (message: string) =>
    new HarnessError('HARNESS_BAD_REQUEST', message)
  let value: unknown
  try {
    value = JSON.parse(raw)
  } catch {
    throw bad('not a JSON object')
  }
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw bad('not a JSON object')
  }
  const request = value as Json
  if (request.v !== PROTOCOL_VERSION) {
    throw bad(`expected protocol version ${PROTOCOL_VERSION}`)
  }
  if (typeof request.id !== 'string') throw bad('"id" must be a string')
  if (typeof request.method !== 'string') {
    throw bad('"method" must be a string')
  }
  const params = request.params ?? {}
  if (typeof params !== 'object' || Array.isArray(params)) {
    throw bad('"params" must be an object')
  }
  return { id: request.id, method: request.method, params: params as Json }
}
