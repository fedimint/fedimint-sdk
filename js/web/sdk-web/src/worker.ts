import { UniffiAbstractObject, uniffiTypeNameSymbol } from '@ubjs/core'
import { Exception, uniffiInitAsync } from './generated'
import * as generated from './generated/fedimint_sdk'
import { isRefMarker, REF_KEY, walk } from './protocol'
import type { Ref, RefMarker, Request, Response, WireError } from './protocol'

// A dedicated worker's global, typed to what is used here. `lib: ["DOM"]` and the worker lib
// cannot be combined in one tsconfig, and this file needs almost nothing from either.
const scope = globalThis as unknown as {
  postMessage(message: Response): void
  addEventListener(
    type: 'message',
    listener: (event: { data: Request }) => void,
  ): void
  addEventListener(type: 'error', listener: (event: ErrorEvent) => void): void
  addEventListener(
    type: 'unhandledrejection',
    listener: (event: PromiseRejectionEvent) => void,
  ): void
}

const objects = new Map<Ref, UniffiAbstractObject>()
let nextRef = 1
const aborts = new Map<number, AbortController>()

// The module is opened once, before the first call; the player installs the panic hook first.
const ready = uniffiInitAsync(
  new URL('./generated/fedimint_sdk.wasm', import.meta.url),
)

function register(object: UniffiAbstractObject): RefMarker {
  const ref = nextRef++
  objects.set(ref, object)
  const type = (object as unknown as Record<symbol, unknown>)[
    uniffiTypeNameSymbol
  ]
  return { [REF_KEY]: ref, type: typeof type === 'string' ? type : 'SdkObject' }
}

function encode(value: unknown): unknown {
  return walk(value, (v) =>
    v instanceof UniffiAbstractObject ? register(v) : undefined,
  )
}

function decodeArgs(args: unknown[]): unknown[] {
  return walk(args, (v) => {
    if (!isRefMarker(v)) return undefined
    const object = objects.get(v[REF_KEY])
    if (!object) throw new Error(`unknown SDK object handle ${v[REF_KEY]}`)
    return object
  }) as unknown[]
}

function toWireError(error: unknown): WireError {
  if (Exception.hasInner(error)) {
    const inner = Exception.getInner(error)
    return {
      sdk: true,
      code: inner.code(),
      reason: inner.reason(),
      details: inner.details(),
    }
  }
  if (error instanceof Error) {
    return {
      sdk: false,
      name: error.name,
      message: error.message,
      stack: error.stack,
    }
  }
  return { sdk: false, name: 'Error', message: String(error) }
}

function fatal(error: string): void {
  console.error(error)
  scope.postMessage({ kind: 'fatal', error })
}

async function handle(
  request: Extract<Request, { kind: 'static' | 'call' }>,
): Promise<void> {
  // The controller is registered before the first `await`: this function is entered
  // synchronously from the message listener, so an `abort` for this id cannot be dispatched
  // until it is in the map, however long the module still takes to load.
  const controller = request.abortable ? new AbortController() : undefined
  if (controller) aborts.set(request.id, controller)
  try {
    await ready
    const args = decodeArgs(request.args)
    if (controller) args.push({ signal: controller.signal })
    let receiver: unknown
    let fn: unknown
    if (request.kind === 'static') {
      const module = generated as unknown as Record<string, unknown>
      receiver = request.cls === undefined ? undefined : module[request.cls]
      fn =
        request.cls === undefined
          ? module[request.fn]
          : (receiver as Record<string, unknown>)[request.fn]
    } else {
      receiver = objects.get(request.ref)
      if (!receiver) throw new Error(`unknown SDK object handle ${request.ref}`)
      fn = (receiver as Record<string, unknown>)[request.method]
    }
    if (typeof fn !== 'function') {
      const name =
        request.kind === 'static'
          ? `${request.cls ?? ''}.${request.fn}`
          : request.method
      throw new Error(`${name} is not a function of the SDK`)
    }
    const value = await (fn as (...a: unknown[]) => unknown).call(
      receiver,
      ...args,
    )
    scope.postMessage({ kind: 'ok', id: request.id, value: encode(value) })
  } catch (error) {
    scope.postMessage({
      kind: 'err',
      id: request.id,
      error: toWireError(error),
    })
    // A trap means the Rust side panicked (the module is built with `panic = "abort"`): its
    // state cannot be trusted any more, so every other call in flight fails too.
    if (error instanceof WebAssembly.RuntimeError)
      fatal(`SDK wasm trap: ${error.message}`)
  } finally {
    aborts.delete(request.id)
  }
}

scope.addEventListener('message', (event) => {
  const request = event.data
  switch (request.kind) {
    case 'static':
    case 'call':
      void handle(request)
      break
    case 'abort':
      aborts.get(request.id)?.abort()
      break
    case 'release': {
      objects.get(request.ref)?.uniffiDestroy()
      objects.delete(request.ref)
      break
    }
  }
})

// A crash outside a call (the init failing, an async task the SDK spawned trapping) would
// otherwise leave every pending promise on the main thread unsettled.
scope.addEventListener('error', (event) => {
  event.preventDefault()
  fatal(`uncaught error in the SDK worker: ${event.message || 'unknown'}`)
})
scope.addEventListener('unhandledrejection', (event) => {
  event.preventDefault()
  fatal(`unhandled rejection in the SDK worker: ${String(event.reason)}`)
})
