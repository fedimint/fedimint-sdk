// The envelope around every call, as PROTOCOL.md defines it.

import { bool, literal, nullable, obj, str, uint } from './decode'
import { errorValue, ErrorValue } from './values'

export const PROTOCOL_VERSION = 1

export interface Request {
  v: typeof PROTOCOL_VERSION
  id: string
  method: string
  params: object
}

export type Response =
  | { kind: 'ready' }
  | { kind: 'pending'; id: string }
  | { kind: 'ok'; id: string; result: unknown; ms: number }
  | { kind: 'error'; id: string | null; error: ErrorValue; received?: string }

/**
 * One line of JSON with every non-ASCII character escaped as `\uXXXX`, so what
 * a driver types is exactly what the harness parses whatever the keyboard or
 * input method does with other characters.
 */
export function encodeRequest(request: Request): string {
  return JSON.stringify(request).replace(
    /[\u007f-￿]/g,
    (c) => `\\u${c.charCodeAt(0).toString(16).padStart(4, '0')}`,
  )
}

/** Reads the response view. Returns null for text that isn't a response yet
 * (empty, or not JSON), which a caller treats as "keep waiting". */
export function parseResponse(text: string): Response | null {
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    return null
  }
  obj({ v: literal(PROTOCOL_VERSION) })(value, 'response')
  const fields = value as Record<string, unknown>
  if (fields.ready === true) return { kind: 'ready' }
  const id = nullable(str)(fields.id, 'response.id')
  if (fields.pending === true && id !== null) return { kind: 'pending', id }
  if (bool(fields.ok, 'response.ok') && id !== null) {
    return { kind: 'ok', id, result: fields.result, ms: uint(fields.ms, 'ms') }
  }
  const error = errorValue(fields.error, 'response.error')
  const received =
    typeof fields.received === 'string' ? fields.received : undefined
  return { kind: 'error', id, error, received }
}
