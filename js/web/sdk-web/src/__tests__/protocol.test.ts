import { describe, expect, it } from 'vitest'
import { isRefMarker, REF_KEY, splitSignal, walk } from '../protocol'

class Handle {
  constructor(readonly name: string) {}
}
class Tagged {
  readonly tag = 'Quarantined'
  constructor(readonly diagnostic: { code: string }) {}
}
const toMarker = (v: object) =>
  v instanceof Handle ? { [REF_KEY]: 7, type: v.name } : undefined

describe('walk', () => {
  it('replaces objects at any depth and flattens class instances', () => {
    const out = walk(
      {
        notes: new Handle('Notes'),
        list: [new Handle('A')],
        map: new Map([['k', new Handle('B')]]),
        status: new Tagged({ code: 'Storage' }),
        amount: 5n,
        bytes: new Uint8Array([1]),
      },
      toMarker,
    ) as Record<string, unknown>
    expect(isRefMarker(out.notes)).toBe(true)
    expect(isRefMarker((out.list as unknown[])[0])).toBe(true)
    expect(isRefMarker((out.map as Map<string, unknown>).get('k'))).toBe(true)
    expect(out.status).toEqual({
      tag: 'Quarantined',
      diagnostic: { code: 'Storage' },
    })
    expect(Object.getPrototypeOf(out.status)).toBe(Object.prototype)
    expect(out.amount).toBe(5n)
    expect(out.bytes).toBeInstanceOf(Uint8Array)
  })
  it('leaves primitives and undefined alone', () => {
    expect(walk(undefined, toMarker)).toBeUndefined()
    expect(walk('x', toMarker)).toBe('x')
  })
})

describe('splitSignal', () => {
  it('peels a trailing { signal } and only that', () => {
    const signal = new AbortController().signal
    expect(splitSignal([1, { signal }])).toEqual({ args: [1], signal })
    expect(splitSignal([1, { signal: 'no' }])).toEqual({
      args: [1, { signal: 'no' }],
    })
    expect(splitSignal([])).toEqual({ args: [] })
  })
})
