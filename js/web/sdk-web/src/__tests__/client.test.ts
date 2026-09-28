import { describe, expect, it, vi } from 'vitest'
import { WorkerSession } from '../client'
import { SdkError, SessionClosed, WorkerCrashed } from '../errors'
import { REF_KEY } from '../protocol'
import { FakeWorker } from './fake-worker'

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
  it('rejects an already-aborted signal without posting anything', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const controller = new AbortController()
    controller.abort()
    const p = s.callMethod(1, 'balance', [{ signal: controller.signal }])
    await expect(p).rejects.toMatchObject({ name: 'AbortError' })
    expect(w.sent).toHaveLength(0)
  })
  it('stops listening for abort once the call has settled', async () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const controller = new AbortController()
    // The listener is observed directly: a later abort posting nothing would also be explained
    // by the pending-map check inside the listener, so that alone proves no detachment.
    const added = vi.spyOn(controller.signal, 'addEventListener')
    const removed = vi.spyOn(controller.signal, 'removeEventListener')
    const p = s.callMethod(1, 'balance', [{ signal: controller.signal }])
    w.reply({ kind: 'ok', id: 1, value: 5n })
    await p
    expect(added).toHaveBeenCalledTimes(1)
    expect(removed).toHaveBeenCalledTimes(1)
    expect(removed.mock.calls[0]![1]).toBe(added.mock.calls[0]![1])
    controller.abort()
    expect(w.sent).toHaveLength(1)
  })
  it('detaches the abort listeners of every call in flight when the session fails', () => {
    const w = new FakeWorker()
    const s = new WorkerSession(w)
    const controller = new AbortController()
    const removed = vi.spyOn(controller.signal, 'removeEventListener')
    const p = s.callMethod(1, 'balance', [{ signal: controller.signal }])
    p.catch(() => {})
    s.fail(new WorkerCrashed('gone'))
    expect(removed).toHaveBeenCalledTimes(1)
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
