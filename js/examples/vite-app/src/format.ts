import { ErrorCode, SdkError } from '@fedimint/web-sdk'

/** An `Amount` is in millisatoshis: whole satoshis read as sat, anything finer as msat. */
export const msat = (amount: bigint | undefined) =>
  amount === undefined
    ? '—'
    : amount % 1000n === 0n
      ? `${(amount / 1000n).toLocaleString()} sat`
      : `${amount.toLocaleString()} msat`

export const sat = (sats: bigint) => `${sats.toLocaleString()} sat`

/** A `Timestamp` is milliseconds since the Unix epoch. */
export const time = (ms: bigint) => new Date(Number(ms)).toLocaleString()

export const short = (id: string) =>
  id.length > 20 ? `${id.slice(0, 8)}…${id.slice(-8)}` : id

// Records cross from the worker with `bigint`s, `Map`s and `ArrayBuffer`s in them, none of which
// `JSON.stringify` takes as they are.
function replacer(_key: string, value: unknown): unknown {
  if (typeof value === 'bigint') return value.toString()
  if (value instanceof Map) return Object.fromEntries(value)
  if (value instanceof ArrayBuffer) return `<${value.byteLength} bytes>`
  return value
}

export const json = (value: unknown, indent = 2) =>
  JSON.stringify(value, replacer, indent) ?? String(value)

/**
 * A state or other enum as text. Tagged enums cross as `{ tag, inner? }`; a plain one (an ecash
 * send's state) is a number, which `names` turns back into its name.
 */
export function variant(value: unknown, names?: (n: number) => string): string {
  if (value === undefined) return '—'
  if (typeof value === 'number' && names) return names(value)
  if (typeof value === 'object' && value !== null && 'tag' in value) {
    const { tag, inner } = value as { tag: string; inner?: unknown }
    return inner === undefined ? tag : `${tag} ${json(inner, 0)}`
  }
  return json(value, 0)
}

/** An SDK error is identified by its code; anything else by its name. */
export function errorText(error: unknown): string {
  if (error instanceof SdkError)
    return `${ErrorCode[error.code]}: ${error.reason}`
  if (error instanceof Error) return `${error.name}: ${error.message}`
  return String(error)
}
