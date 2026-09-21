import { fromWireError, SessionClosed, WorkerCrashed } from './errors'
import { isRefMarker, REF_KEY, splitSignal, walk } from './protocol'
import type { Ref, RefMarker, Request, Response } from './protocol'

/** The `Worker` surface the session uses, so a test can substitute a fake. */
export interface WorkerLike {
  postMessage(message: unknown): void
  addEventListener(
    type: 'message',
    listener: (event: { data: unknown }) => void,
  ): void
  addEventListener(
    type: 'error',
    listener: (event: { message?: string }) => void,
  ): void
  addEventListener(type: 'messageerror', listener: () => void): void
  terminate(): void
}

interface Pending {
  resolve(value: unknown): void
  reject(error: Error): void
  /** Detaches this call's `abort` listener from its signal once the call has settled. */
  cleanup(): void
}

type CallHead =
  | { kind: 'static'; cls?: string; fn: string }
  | { kind: 'call'; ref: Ref; method: string }

/**
 * The main-thread end of the worker protocol: sends calls, keeps the promises they return, and
 * turns object handles into proxies whose every property is an async method.
 */
export class WorkerSession {
  private readonly pending = new Map<number, Pending>()
  private nextId = 1
  private dead: Error | undefined
  private readonly refOf = new WeakMap<object, Ref>()
  // A proxy the application dropped releases the worker's object.
  private readonly collected = new FinalizationRegistry<Ref>((ref) => {
    if (!this.dead)
      this.worker.postMessage({ kind: 'release', ref } satisfies Request)
  })

  constructor(private readonly worker: WorkerLike) {
    worker.addEventListener('message', (event) =>
      this.onMessage(event.data as Response),
    )
    worker.addEventListener('error', (event) => {
      this.fail(
        new WorkerCrashed(`SDK worker error: ${event.message || 'unknown'}`),
      )
    })
    worker.addEventListener('messageerror', () => {
      this.fail(
        new WorkerCrashed(
          'SDK worker sent a message that could not be decoded',
        ),
      )
    })
  }

  callStatic(
    cls: string | undefined,
    fn: string,
    args: unknown[],
  ): Promise<unknown> {
    return this.request({ kind: 'static', cls, fn }, args)
  }

  callMethod(ref: Ref, method: string, args: unknown[]): Promise<unknown> {
    return this.request({ kind: 'call', ref, method }, args)
  }

  /** Fails every call in flight and every call made afterwards. */
  fail(error: Error): void {
    if (this.dead) return
    this.dead = error
    for (const pending of this.pending.values()) {
      pending.cleanup()
      pending.reject(error)
    }
    this.pending.clear()
  }

  terminate(): void {
    this.worker.terminate()
    this.fail(new SessionClosed())
  }

  private request(head: CallHead, rawArgs: unknown[]): Promise<unknown> {
    if (this.dead) return Promise.reject(this.dead)
    const { args, signal } = splitSignal(rawArgs)
    // An already-aborted signal never fires its `abort` event, so without this check the call
    // would still be posted to the worker and run to completion.
    if (signal?.aborted)
      return Promise.reject(
        new DOMException('The operation was aborted', 'AbortError'),
      )
    const id = this.nextId++
    return new Promise((resolve, reject) => {
      const onAbort = () => {
        if (this.pending.has(id))
          this.worker.postMessage({ kind: 'abort', id } satisfies Request)
      }
      signal?.addEventListener('abort', onAbort)
      this.pending.set(id, {
        resolve,
        reject,
        cleanup: () => signal?.removeEventListener('abort', onAbort),
      })
      this.worker.postMessage({
        ...head,
        id,
        args: walk(args, (v) => this.markerFor(v)),
        abortable: signal !== undefined,
      } as Request)
    })
  }

  private markerFor(value: object): RefMarker | undefined {
    const ref = this.refOf.get(value)
    return ref === undefined
      ? undefined
      : { [REF_KEY]: ref, type: String(value) }
  }

  private onMessage(message: Response): void {
    if (message.kind === 'fatal') {
      this.fail(new WorkerCrashed(message.error))
      return
    }
    const pending = this.pending.get(message.id)
    if (!pending) return
    this.pending.delete(message.id)
    pending.cleanup()
    if (message.kind === 'ok') {
      pending.resolve(
        walk(message.value, (v) =>
          isRefMarker(v) ? this.proxyFor(v) : undefined,
        ),
      )
    } else {
      pending.reject(fromWireError(message.error))
    }
  }

  private proxyFor(marker: RefMarker): object {
    const ref = marker[REF_KEY]
    const proxy: object = new Proxy(Object.create(null), {
      get: (_, prop) => {
        if (prop === Symbol.toStringTag) return marker.type
        // `toString`, `valueOf` and `toJSON` are what the language's implicit coercion
        // (`String(proxy)`, `` `${proxy}` ``, `+proxy`) and `JSON.stringify` read on their own,
        // without the caller ever awaiting anything. Left to fall through, they would forward
        // to the worker like any other property, firing an RPC the caller never awaits, which
        // the worker then answers with "is not a function of the SDK". Answering all three
        // locally with the same synchronous string keeps a handle from ever starting an RPC by
        // being printed or serialised.
        if (prop === 'toString' || prop === 'valueOf' || prop === 'toJSON') {
          return () => `[object ${marker.type}]`
        }
        // Never thenable: `await` on a handle must hand the handle back, not call into it.
        if (prop === 'then' || typeof prop !== 'string') return undefined
        return (...args: unknown[]) => this.callMethod(ref, prop, args)
      },
      has: (_, prop) => typeof prop === 'string',
    })
    this.refOf.set(proxy, ref)
    this.collected.register(proxy, ref)
    return proxy
  }
}
