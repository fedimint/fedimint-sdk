/**
 * The main-thread view of a generated SDK type. Objects live in the worker, so every method
 * becomes asynchronous and every object in a signature becomes a `Proxied` handle; records,
 * enums, `bigint`s, strings and byte arrays cross by value.
 */
export type Proxied<T> = {
  [K in keyof T as T[K] extends (...args: never[]) => unknown
    ? K
    : never]: T[K] extends (...args: infer A) => infer R
    ? (...args: ProxiedArgs<A>) => Promise<ProxiedValue<Awaited<R>>>
    : never
}

type ProxiedArgs<A> = { [I in keyof A]: ProxiedValue<A[I]> }

type HasMethod<T> = {
  [K in keyof T]-?: T[K] extends (...args: never[]) => unknown ? true : never
}[keyof T]

/**
 * The data fields of a tagged enum variant: what the worker's deep walk leaves of it. The
 * generated variant interfaces carry a symbol-keyed `[uniffiTypeNameSymbol]` property alongside
 * `tag` and any data; `walk` only ever copies string-keyed own properties (`Object.entries`
 * skips symbols), so that property never reaches the wire and is excluded here too.
 */
type Fields<V> = {
  [K in keyof V as K extends string
    ? V[K] extends (...args: never[]) => unknown
      ? never
      : K
    : never]: ProxiedValue<V[K]>
}

/**
 * Objects (anything with a method) become handles; tagged enum variants (anything with a `tag`)
 * keep their data fields; everything else is mapped structurally.
 */
export type ProxiedValue<V> = V extends
  | bigint
  | string
  | number
  | boolean
  | null
  | undefined
  ? V
  : V extends Uint8Array | ArrayBuffer | Date
    ? V
    : V extends Array<infer E>
      ? Array<ProxiedValue<E>>
      : V extends Map<infer K, infer W>
        ? Map<ProxiedValue<K>, ProxiedValue<W>>
        : V extends { readonly tag: string }
          ? Fields<V>
          : V extends object
            ? HasMethod<V> extends never
              ? { [K in keyof V]: ProxiedValue<V[K]> }
              : Proxied<V>
            : V
