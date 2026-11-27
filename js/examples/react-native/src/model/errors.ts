import { ErrorCode, Exception } from '@fedimint/react-native'

/** The SDK's error code and reason, when `error` came from the SDK. */
export function sdkError(
  error: unknown,
): { code: ErrorCode; reason: string } | undefined {
  if (!Exception.hasInner(error)) return undefined
  const inner = Exception.getInner(error)
  return { code: inner.code(), reason: inner.reason() }
}

/** A code's name in camelCase: `unsupportedFederation`. */
export function codeName(code: ErrorCode): string {
  const name = ErrorCode[code]
  return name.charAt(0).toLowerCase() + name.slice(1)
}

/** The reason alone, for a line that already says what stopped. */
export function reason(error: unknown): string {
  const sdk = sdkError(error)
  return sdk ? sdk.reason : message(error)
}

/**
 * What a failed action says: a sentence for the errors a user can act on, otherwise the code and
 * its reason.
 */
export function describe(error: unknown): string {
  const sdk = sdkError(error)
  if (!sdk) return message(error)
  switch (sdk.code) {
    case ErrorCode.InvalidInput:
      return `That input isn't valid.\n\n${sdk.reason}`
    case ErrorCode.AlreadyJoined:
      return "You're already in this federation."
    case ErrorCode.FederationUnreachable:
    case ErrorCode.Timeout:
      return `Couldn't reach the federation.\n\n${sdk.reason}`
    case ErrorCode.UnsupportedFederation:
      return "This app can't work with that federation."
    case ErrorCode.InsufficientBalance:
      return `Not enough balance.\n\n${sdk.reason}`
    case ErrorCode.QuoteExpired:
      return `That quote is no longer usable. Quote again.\n\n${sdk.reason}`
    case ErrorCode.Recovering:
      return 'Still recovering. Try again when the rescan finishes.'
    default:
      return `${codeName(sdk.code)}: ${sdk.reason}`
  }
}

/** An error's message, whatever was thrown. */
function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

/** Whether `error` is the rejection an aborted SDK call ends with. */
export function isAbort(error: unknown): boolean {
  return error instanceof Error && error.name === 'AbortError'
}
