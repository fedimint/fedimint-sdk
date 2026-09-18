import { describe, expect, it } from 'vitest'
import { WorkerSession } from '../client'
import type { WorkerLike } from '../client'
import { SdkError, SessionClosed, WorkerCrashed } from '../errors'
import { REF_KEY } from '../protocol'
import type { Request, Response } from '../protocol'

class FakeWorker implements WorkerLike {
  sent: Request[] = []
  private listeners = new Map<string, ((event: never) => void)[]>()
  terminated = false
  postMessage(message: unknown) {
    this.sent.push(message as Request)
  }
  addEventListener(type: string, listener: (event: never) => void) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener])
  }
  terminate() {
    this.terminated = true
  }
  reply(message: Response) {
    for (const l of this.listeners.get('message') ?? [])
      (l as (e: { data: Response }) => void)({ data: message })
  }
  crash(message: string) {
    for (const l of this.listeners.get('error') ?? [])
      (l as (e: { message: string }) => void)({ message })
  }
}

describe('WorkerSession', () => {
  it('resolves a call with proxies for handles, nested too', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const p = s.callStatic(undefined, 'createFedimintSdk', ['store', undefined])
    expect(w.sent[0]).toMatchObject({
      kind: 'static',
      fn: 'createFedimintSdk',
      id: 1,
      abortable: false,
    })
    w.reply({ kind: 'ok', id: 1, value: { [REF_KEY]: 3, type: 'Sdk' } })
    const sdk = (await p) as Record<
      string,
      (...a: unknown[]) => Promise<unknown>
    >
    expect(String(sdk)).toBe('[object Sdk]')
    const q = sdk.join({ [REF_KEY]: 9, type: 'InviteCode' })
    expect(w.sent[1]).toMatchObject({
      kind: 'call',
      ref: 3,
      method: 'join',
      id: 2,
    })
    w.reply({
      kind: 'ok',
      id: 2,
      value: {
        invoice: 'lnbc1',
        operation: { [REF_KEY]: 4, type: 'LnReceiveOperation' },
      },
    })
    const handle = (await q) as { invoice: string; operation: object }
    expect(handle.invoice).toBe('lnbc1')
    expect(String(handle.operation)).toBe('[object LnReceiveOperation]')
  })
  it('sends a proxy back as its handle', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const p = s.callStatic('InviteCode', 'parse', ['fed1...'])
    w.reply({ kind: 'ok', id: 1, value: { [REF_KEY]: 5, type: 'InviteCode' } })
    const invite = await p
    void s.callMethod(3, 'join', [invite])
    expect(w.sent[1]).toMatchObject({
      kind: 'call',
      args: [{ [REF_KEY]: 5, type: '[object InviteCode]' }],
    })
  })
  it('rejects with SdkError carrying the code', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const p = s.callStatic('InviteCode', 'parse', ['nope'])
    w.reply({
      kind: 'err',
      id: 1,
      error: {
        sdk: true,
        code: 'InvalidInput' as never,
        reason: 'invalid invite code',
      },
    })
    await expect(p).rejects.toMatchObject({
      name: 'SdkError',
      code: 'InvalidInput',
      reason: 'invalid invite code',
    })
    await expect(p).rejects.toBeInstanceOf(SdkError)
  })
  it('fails every pending call when the worker dies, and every later one', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const a = s.callMethod(1, 'balance', [])
    const b = s.callMethod(1, 'name', [])
    w.crash('boom')
    await expect(a).rejects.toBeInstanceOf(WorkerCrashed)
    await expect(b).rejects.toBeInstanceOf(WorkerCrashed)
    await expect(s.callMethod(1, 'id', [])).rejects.toBeInstanceOf(
      WorkerCrashed,
    )
  })
  it('fatal messages fail pending calls too', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const a = s.callMethod(1, 'balance', [])
    w.reply({ kind: 'fatal', error: 'SDK wasm trap: unreachable' })
    await expect(a).rejects.toThrow('SDK wasm trap: unreachable')
  })
  it('terminate closes the worker and rejects with SessionClosed', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const a = s.callMethod(1, 'balance', [])
    s.terminate()
    expect(w.terminated).toBe(true)
    await expect(a).rejects.toBeInstanceOf(SessionClosed)
  })
  it('forwards an abort for the call it belongs to', () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const controller = new AbortController()
    void s.callMethod(1, 'balance', [{ signal: controller.signal }])
    expect(w.sent[0]).toMatchObject({ abortable: true, args: [] })
    controller.abort()
    expect(w.sent[1]).toEqual({ kind: 'abort', id: 1 })
  })
  it('a handle is not thenable', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const p = s.callStatic(undefined, 'createFedimintSdk', ['s', undefined])
    w.reply({ kind: 'ok', id: 1, value: { [REF_KEY]: 3, type: 'Sdk' } })
    const sdk = await p
    expect(await Promise.resolve(sdk)).toBe(sdk)
  })
  it('coerces to a primitive without sending a message, and still awaits', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const p = s.callStatic(undefined, 'createFedimintSdk', ['s', undefined])
    w.reply({ kind: 'ok', id: 1, value: { [REF_KEY]: 3, type: 'Sdk' } })
    const sdk = await p
    expect(`${sdk}`).toBe('[object Sdk]')
    expect(String(sdk)).toBe('[object Sdk]')
    expect(JSON.stringify({ handle: sdk })).toBe('{"handle":"[object Sdk]"}')
    // The number hint reads valueOf first, the path the other two never reach.
    expect(+(sdk as unknown as number)).toBeNaN()
    expect(`x: ${sdk}`).toBe('x: [object Sdk]')
    expect(w.sent).toHaveLength(1)
    expect(await Promise.resolve(sdk)).toBe(sdk)
  })
})
