import { ErrorValue } from '../protocol/values'

/** A call the harness answered with `ok:false`: an SDK error (its ErrorCode)
 * or a harness one (HARNESS_*). Tests branch on `code`. */
export class HarnessCallError extends Error {
  readonly code: string
  readonly reason: string

  constructor(
    readonly method: string,
    error: ErrorValue,
  ) {
    super(`${method} failed with [${error.code}] ${error.reason}`)
    this.code = error.code
    this.reason = error.reason
  }
}

/** The console itself misbehaved: no response, a corrupted request, or a
 * result that doesn't match PROTOCOL.md. Never an SDK outcome. */
export class TransportError extends Error {}
