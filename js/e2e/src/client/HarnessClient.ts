/* eslint-disable no-console */
import { randomBytes } from 'crypto'

import { ConsoleBackend } from '../backends/types'
import { DecodeError } from '../protocol/decode'
import {
  encodeRequest,
  parseResponse,
  PROTOCOL_VERSION,
  Response,
} from '../protocol/envelope'
import { Method, ParamsOf, RESULTS, ResultOf } from '../protocol/methods'
import { ErrorValue } from '../protocol/values'

import { HarnessCallError, TransportError } from './errors'

/** The default budget for one call: long enough for a join or a first open
 * on a cold device, which are the slowest calls that return promptly. */
export const CALL_TIMEOUT = 60_000

const POLL_MS = 250
/** With no pending marker by then, the tap is taken as lost and repeated. A
 * repeat is safe: the harness answers a repeated id from its cache. */
const RETAP_AFTER_MS = 5_000

export type Outcome<T> =
  | { ok: true; result: T }
  | { ok: false; error: ErrorValue }

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

/** Calls protocol methods on a harness through its console. */
export class HarnessClient {
  private seq = 0
  // Fresh per client, so a response left on screen by an earlier run or
  // before a restart never matches a request from this one.
  private readonly nonce = randomBytes(3).toString('hex')

  constructor(
    private readonly backend: ConsoleBackend,
    private readonly log: (line: string) => void = console.log,
  ) {}

  /** Calls `method` and returns its decoded result, or throws
   * HarnessCallError when the harness answers with an error. */
  async call<M extends Method>(
    method: M,
    params: ParamsOf<M>,
    opts: { timeoutMs?: number } = {},
  ): Promise<ResultOf<M>> {
    const outcome = await this.try(method, params, opts)
    if (!outcome.ok) throw new HarnessCallError(method, outcome.error)
    return outcome.result
  }

  /** Like `call`, but an error answer is returned rather than thrown, for
   * tests that expect one. */
  async try<M extends Method>(
    method: M,
    params: ParamsOf<M>,
    opts: { timeoutMs?: number } = {},
  ): Promise<Outcome<ResultOf<M>>> {
    const response = await this.exchange(
      method,
      params,
      opts.timeoutMs ?? CALL_TIMEOUT,
    )
    if (response.kind === 'error') return { ok: false, error: response.error }
    try {
      return {
        ok: true,
        result: RESULTS[method](response.result, 'result') as ResultOf<M>,
      }
    } catch (error) {
      if (!(error instanceof DecodeError)) throw error
      throw new TransportError(
        `${method} answered outside PROTOCOL.md — ${error.message}\n` +
          `raw result: ${JSON.stringify(response.result)}`,
      )
    }
  }

  /** Sends any method name and params, unchecked, and returns the raw outcome.
   * Only for conformance tests that must send what the types forbid. */
  async tryUnchecked(
    method: string,
    params: object,
  ): Promise<Outcome<unknown>> {
    const response = await this.exchange(method, params, CALL_TIMEOUT)
    return response.kind === 'ok'
      ? { ok: true, result: response.result }
      : { ok: false, error: response.error }
  }

  /**
   * Calls `method` until `accept` returns something other than undefined, and
   * returns that. For waiting on a state the harness records as it happens
   * (an operation's states, the observed balance).
   */
  async poll<M extends Method, T>(
    method: M,
    params: ParamsOf<M>,
    accept: (result: ResultOf<M>) => T | undefined,
    opts: { timeoutMs: number; intervalMs?: number; what: string },
  ): Promise<T> {
    const deadline = Date.now() + opts.timeoutMs
    let last: ResultOf<M> | undefined
    while (Date.now() < deadline) {
      last = await this.call(method, params)
      const accepted = accept(last)
      if (accepted !== undefined) return accepted
      await sleep(opts.intervalMs ?? 1000)
    }
    throw new Error(
      `Timed out after ${opts.timeoutMs}ms waiting for ${opts.what} — last ${method}: ` +
        JSON.stringify(last),
    )
  }

  private async exchange(
    method: string,
    params: object,
    timeoutMs: number,
  ): Promise<Response & { kind: 'ok' | 'error' }> {
    const id = `${this.nonce}:${++this.seq}`
    const json = encodeRequest({ v: PROTOCOL_VERSION, id, method, params })
    await this.write(json)
    await this.backend.run()

    const start = Date.now()
    let tappedAt = start
    let acknowledged = false
    let lastText = ''
    while (Date.now() - start < timeoutMs) {
      lastText = await this.backend.readResponse()
      const response = parseResponse(lastText)
      if (response?.kind === 'error' && response.id === null) {
        throw new TransportError(
          `The harness could not read the ${method} request: ${response.error.reason}` +
            (response.received ? ` (received: ${response.received})` : ''),
        )
      }
      if (response && 'id' in response && response.id === id) {
        if (response.kind === 'pending') acknowledged = true
        else if (response.kind === 'ok' || response.kind === 'error') {
          return response
        }
      }
      if (!acknowledged && Date.now() - tappedAt > RETAP_AFTER_MS) {
        this.log(`[client] no answer to ${method} yet, tapping run again`)
        await this.backend.run()
        tappedAt = Date.now()
      }
      await sleep(POLL_MS)
    }
    throw new TransportError(
      `No answer to ${method} (${id}) within ${timeoutMs}ms` +
        (acknowledged ? ' after the harness acknowledged it' : '') +
        ` — response view: ${lastText.slice(0, 300)}`,
    )
  }

  private async write(json: string): Promise<void> {
    if (!this.backend.verifyInput) {
      await this.backend.writeRequest(json)
      return
    }
    for (let attempt = 1; attempt <= 3; attempt++) {
      await this.backend.writeRequest(json)
      const typed = await this.backend.readRequest()
      if (typed === json) return
      this.log(
        `[client] request field differs from what was typed (attempt ${attempt})`,
      )
    }
    throw new TransportError(
      `Could not type the request intact: ${json.slice(0, 200)}`,
    )
  }
}
