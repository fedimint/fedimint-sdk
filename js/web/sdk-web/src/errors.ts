import type { ErrorCode, RawErrorDetails } from './generated/fedimint_sdk'
import type { WireError } from './protocol'

/** An error the SDK returned: branch on `code`, never on `message`. */
export class SdkError extends Error {
  constructor(
    readonly code: ErrorCode,
    readonly reason: string,
    readonly details?: RawErrorDetails,
  ) {
    super(reason)
    this.name = 'SdkError'
  }
}

/** The worker hosting the SDK died; every call in flight fails with this. */
export class WorkerCrashed extends Error {
  constructor(message: string) {
    super(message)
    this.name = 'WorkerCrashed'
  }
}

/** `close()` was called; every call made after it fails with this. */
export class SessionClosed extends Error {
  constructor() {
    super('the SDK session is closed')
    this.name = 'SessionClosed'
  }
}

export function fromWireError(error: WireError): Error {
  if (error.sdk) return new SdkError(error.code, error.reason, error.details)
  const err = new Error(error.message)
  err.name = error.name
  if (error.stack) err.stack = error.stack
  return err
}
