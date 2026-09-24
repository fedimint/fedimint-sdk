import type { ErrorCode, RawErrorDetails } from './generated/fedimint_sdk'

/** A handle to an SDK object living in the worker. */
export type Ref = number

export const REF_KEY = '__fedimintSdkRef'

/** How an object handle travels: a marker object in place of the object. */
export interface RefMarker {
  [REF_KEY]: Ref
  /** The object's type name, for diagnostics and `Symbol.toStringTag`. */
  type: string
}

export type Request =
  | {
      kind: 'static'
      id: number
      cls?: string
      fn: string
      args: unknown[]
      abortable: boolean
    }
  | {
      kind: 'call'
      id: number
      ref: Ref
      method: string
      args: unknown[]
      abortable: boolean
    }
  | { kind: 'abort'; id: number }
  | { kind: 'release'; ref: Ref }

export type WireError =
  | { sdk: true; code: ErrorCode; reason: string; details?: RawErrorDetails }
  | { sdk: false; name: string; message: string; stack?: string }

export type Response =
  | { kind: 'ok'; id: number; value: unknown }
  | { kind: 'err'; id: number; error: WireError }
  | { kind: 'fatal'; error: string }

export function isRefMarker(value: unknown): value is RefMarker {
  return typeof value === 'object' && value !== null && REF_KEY in value
}

/**
 * Deep-copies `value`, letting `replace` swap any object it recognises. Arrays, maps and plain
 * objects are walked; class instances are flattened to their own enumerable properties, which
 * is what turns a tagged enum instance into `{ tag, ...fields }`. Typed arrays, buffers, dates
 * and primitives (including `bigint`) pass through untouched.
 */
export function walk(value: unknown, replace: (v: object) => unknown): unknown {
  if (value === null || typeof value !== 'object') return value
  const replaced = replace(value)
  if (replaced !== undefined) return replaced
  if (Array.isArray(value)) return value.map((v) => walk(v, replace))
  if (value instanceof Map) {
    return new Map(
      [...value].map(([k, v]) => [walk(k, replace), walk(v, replace)]),
    )
  }
  if (
    ArrayBuffer.isView(value) ||
    value instanceof ArrayBuffer ||
    value instanceof Date
  ) {
    return value
  }
  const out: Record<string, unknown> = {}
  for (const [k, v] of Object.entries(value)) out[k] = walk(v, replace)
  return out
}

/** Splits a trailing `{ signal }` options argument off a call's argument list. */
export function splitSignal(args: unknown[]): {
  args: unknown[]
  signal?: AbortSignal
} {
  const last = args[args.length - 1]
  if (
    typeof last === 'object' &&
    last !== null &&
    'signal' in last &&
    (last as { signal: unknown }).signal instanceof AbortSignal
  ) {
    return {
      args: args.slice(0, -1),
      signal: (last as { signal: AbortSignal }).signal,
    }
  }
  return { args }
}
