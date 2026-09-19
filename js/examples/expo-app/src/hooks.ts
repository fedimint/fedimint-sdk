import { useCallback, useEffect, useState } from 'react'
import { current, errorMessage, federation, msatToSat, onChange, open, walletExists } from './sdk'

type WalletState = { exists: boolean | undefined; joined: boolean }

/**
 * Tracks whether a wallet has been created on this device and whether it has
 * joined a federation. On mount it opens an existing wallet, if any;
 * `refresh` re-checks after an action that may have changed either. It also
 * re-checks on every `onChange` notification (another screen opened, restored
 * or reset the session), so every screen using this hook stays in sync;
 * `version` counts those notifications for `useBalance` to key its subscription on.
 */
export const useWallet = () => {
  const [state, setState] = useState<WalletState>({
    exists: undefined,
    joined: false,
  })
  const [version, setVersion] = useState(0)

  const refresh = useCallback(() => {
    setState({ exists: true, joined: federation() !== undefined })
  }, [])

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      const exists = await walletExists()
      if (cancelled) return
      if (!exists) {
        setState({ exists: false, joined: false })
        return
      }
      try {
        await open()
        if (!cancelled) {
          setState({ exists: true, joined: federation() !== undefined })
        }
      } catch {
        if (!cancelled) setState({ exists: true, joined: false })
      }
    }
    load()
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(
    () =>
      onChange(() => {
        setState({ exists: current() !== undefined, joined: federation() !== undefined })
        setVersion((v) => v + 1)
      }),
    [],
  )

  return { ...state, refresh, version }
}

/** The joined federation's balance in sats, kept live through `balanceUpdates`. */
export const useBalance = (joined: boolean, version: number): number => {
  const [balance, setBalance] = useState(0)

  useEffect(() => {
    if (!joined) {
      setBalance(0)
      return
    }
    const fed = federation()
    if (!fed) return

    const controller = new AbortController()
    let cancelled = false

    const subscribe = async () => {
      try {
        const initial = await fed.balance()
        if (cancelled) return
        setBalance(msatToSat(initial))
        const updates = fed.balanceUpdates()
        for (;;) {
          const next = await updates.next({ signal: controller.signal })
          if (cancelled) return
          setBalance(msatToSat(next))
        }
      } catch (error) {
        if (!controller.signal.aborted) {
          console.warn('balance updates stopped:', errorMessage(error))
        }
      }
    }
    subscribe()

    return () => {
      cancelled = true
      controller.abort()
    }
    // `version` restarts the loop against the new session on every open/reset, so a
    // closed session's iterator is never awaited past the session that owned it.
  }, [joined, version])

  return balance
}
