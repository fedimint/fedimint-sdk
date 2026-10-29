import {
  ErrorCode,
  SdkError,
  type FederationLike,
  type Proxied,
} from '@fedimint/web-sdk'
import { useCallback, useEffect, useState } from 'react'
import { errorText, variant } from './format'

/**
 * Runs one asynchronous action at a time for a section, keeping it disabled while the action
 * runs and showing the failure, named by its SDK error code when it has one.
 */
export function useAction() {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const run = useCallback(
    async <T>(action: () => Promise<T>): Promise<T | undefined> => {
      setBusy(true)
      setError('')
      try {
        return await action()
      } catch (e) {
        setError(errorText(e))
        return undefined
      } finally {
        setBusy(false)
      }
    },
    [],
  )
  return { run, busy, error, setError }
}

/** The federation's balance, kept current: each `next()` waits for the balance to change. */
export function useBalance(
  federation: Proxied<FederationLike> | undefined,
): bigint | undefined {
  const [balance, setBalance] = useState<bigint>()
  useEffect(() => {
    setBalance(undefined)
    if (!federation) return
    const controller = new AbortController()
    void (async () => {
      try {
        const first = await federation.balance()
        if (controller.signal.aborted) return
        setBalance(first)
        const updates = await federation.balanceUpdates()
        for (;;) setBalance(await updates.next({ signal: controller.signal }))
      } catch (error) {
        // Closing the federation ends its subscription; nothing to report then.
        const closed =
          error instanceof SdkError && error.code === ErrorCode.FederationClosed
        if (!controller.signal.aborted && !closed)
          console.error('balance:', error)
      }
    })()
    return () => controller.abort()
  }, [federation])
  return balance
}

/**
 * What every operation handle has, whatever its kind. `updates()` ends, with `undefined`, once
 * the operation is final.
 */
export interface OperationLike {
  state(): Promise<unknown>
  updates(): Promise<{
    next(options?: { signal: AbortSignal }): Promise<unknown>
  }>
}

/** Follows an operation's state until it is final. */
export function useOperation(
  operation: OperationLike | undefined,
  names?: (n: number) => string,
): { state: string | undefined; final: boolean } {
  const [state, setState] = useState<string>()
  const [final, setFinal] = useState(false)
  useEffect(() => {
    setState(undefined)
    setFinal(false)
    if (!operation) return
    const controller = new AbortController()
    const signal = controller.signal
    void (async () => {
      try {
        // The first state of an operation that has since been replaced is not shown.
        const first = await operation.state()
        if (signal.aborted) return
        setState(variant(first, names))
        const updates = await operation.updates()
        for (;;) {
          const next = await updates.next({ signal })
          // An ecash send's state is a plain enum whose first value is 0, so the end is told by
          // `undefined`, not falsiness.
          if (next === undefined) break
          setState(variant(next, names))
        }
        setFinal(true)
      } catch (error) {
        if (!signal.aborted) setState(errorText(error))
      }
    })()
    return () => controller.abort()
    // `names` is a module-level function where it is given.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [operation])
  return { state, final }
}

/**
 * A quote and the input it was made for. `quote` and `info` are there only while the form still
 * shows that input, so a quote that comes back after an edit can never be sent.
 */
export function useQuote<Q>(input: string) {
  const [quoted, setQuoted] = useState<{
    input: string
    quote: Q
    info: [string, string][]
  }>()
  const current = quoted?.input === input ? quoted : undefined
  return {
    quote: current?.quote,
    info: current?.info ?? [],
    set: (input: string, quote: Q, info: [string, string][]) =>
      setQuoted({ input, quote, info }),
    clear: () => setQuoted(undefined),
  }
}
