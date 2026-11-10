// Small decoders that check a harness's JSON as they read it. Each returns the
// typed value or throws a DecodeError naming the path, so a harness that drifts
// from PROTOCOL.md fails at the field that drifted.

export class DecodeError extends Error {
  constructor(
    readonly path: string,
    readonly expected: string,
    readonly actual: unknown,
  ) {
    super(`${path}: expected ${expected}, got ${JSON.stringify(actual)}`)
  }
}

export type Decoder<T> = (value: unknown, path: string) => T
export type Decoded<D> = D extends Decoder<infer T> ? T : never

export const str: Decoder<string> = (v, p) => {
  if (typeof v !== 'string') throw new DecodeError(p, 'a string', v)
  return v
}

export const bool: Decoder<boolean> = (v, p) => {
  if (typeof v !== 'boolean') throw new DecodeError(p, 'a boolean', v)
  return v
}

/** A non-negative safe integer: amounts, counts, timestamps. */
export const uint: Decoder<number> = (v, p) => {
  if (typeof v !== 'number' || !Number.isSafeInteger(v) || v < 0) {
    throw new DecodeError(p, 'a non-negative safe integer', v)
  }
  return v
}

export const hex = (bytes: number): Decoder<string> => {
  const re = new RegExp(`^[0-9a-f]{${bytes * 2}}$`)
  return (v, p) => {
    if (typeof v !== 'string' || !re.test(v)) {
      throw new DecodeError(p, `${bytes} bytes of lowercase hex`, v)
    }
    return v
  }
}

export function literal<T extends string | number | boolean>(
  expected: T,
): Decoder<T> {
  return (v, p) => {
    if (v !== expected) throw new DecodeError(p, JSON.stringify(expected), v)
    return expected
  }
}

export function oneOf<T extends string>(values: readonly T[]): Decoder<T> {
  return (v, p) => {
    if (typeof v !== 'string' || !(values as readonly string[]).includes(v)) {
      throw new DecodeError(p, `one of ${values.join(', ')}`, v)
    }
    return v as T
  }
}

/** The key must be present; null is the protocol's "none". */
export function nullable<T>(inner: Decoder<T>): Decoder<T | null> {
  return (v, p) => (v === null ? null : inner(v, p))
}

export function arr<T>(inner: Decoder<T>): Decoder<T[]> {
  return (v, p) => {
    if (!Array.isArray(v)) throw new DecodeError(p, 'an array', v)
    return v.map((item, i) => inner(item, `${p}[${i}]`))
  }
}

export function record<T>(inner: Decoder<T>): Decoder<Record<string, T>> {
  return (v, p) => {
    if (typeof v !== 'object' || v === null || Array.isArray(v)) {
      throw new DecodeError(p, 'an object', v)
    }
    const out: Record<string, T> = {}
    for (const [k, item] of Object.entries(v)) out[k] = inner(item, `${p}.${k}`)
    return out
  }
}

type Shape = Record<string, Decoder<unknown>>
type FromShape<S extends Shape> = { [K in keyof S]: Decoded<S[K]> }

/** Every key in `shape` must be present. Extra keys are allowed, so a harness
 * may add fields before the suite reads them. */
export function obj<S extends Shape>(shape: S): Decoder<FromShape<S>> {
  return (v, p) => {
    if (typeof v !== 'object' || v === null || Array.isArray(v)) {
      throw new DecodeError(p, 'an object', v)
    }
    const input = v as Record<string, unknown>
    const out: Record<string, unknown> = {}
    for (const [k, decode] of Object.entries(shape)) {
      if (!(k in input))
        throw new DecodeError(`${p}.${k}`, 'a value', undefined)
      out[k] = decode(input[k], `${p}.${k}`)
    }
    return out as FromShape<S>
  }
}

/** A `{type, ...fields}` value whose fields depend on `type`. */
export function tagged<V extends Record<string, Shape>>(
  variants: V,
): Decoder<{ [T in keyof V]: { type: T } & FromShape<V[T]> }[keyof V]> {
  const types = Object.keys(variants)
  return (v, p) => {
    const type = obj({ type: oneOf(types) })(v, p).type
    return { type, ...obj(variants[type])(v, p) } as never
  }
}
