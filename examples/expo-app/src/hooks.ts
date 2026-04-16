import { useCallback, useEffect, useState } from 'react'
import { walletReady } from './wallet'

export const useIsOpen = () => {
  const [open, setIsOpen] = useState(false)

  const checkIsOpen = useCallback(async () => {
    try {
      const w = await walletReady
      setIsOpen(w.isOpen())
    } catch {
      // wallet not yet ready
    }
  }, [])

  useEffect(() => {
    const tryOpen = async () => {
      try {
        const w = await walletReady
        if (!w.isOpen()) {
          console.log('Attempting to open wallet on startup...')
          await w.open()
        }
        setIsOpen(w.isOpen())
      } catch (e) {
        console.log(
          'Wallet could not be opened on startup (might not be joined): ',
          e,
        )
      }
    }
    tryOpen()
  }, [])

  return { open, checkIsOpen }
}

export const useBalance = (checkIsOpen: () => void | Promise<void>) => {
  const [balance, setBalance] = useState(0)

  useEffect(() => {
    let unsubscribe: (() => void) | undefined
    let cancelled = false
    walletReady.then(async (w) => {
      // Wait until the wallet is actually open (joined to a federation)
      await w.waitForOpen()
      if (cancelled) return
      // Fetch current balance immediately
      const current = await w.balance.getBalance()
      if (!cancelled && typeof current === 'number') setBalance(current)
      // Then subscribe to changes
      unsubscribe = w.balance.subscribeBalance((bal: number) => {
        checkIsOpen()
        setBalance(bal)
      })
    })

    return () => {
      cancelled = true
      unsubscribe?.()
    }
  }, [checkIsOpen])

  return balance
}

export const extractErrorMessage = (error: any): string => {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  if (typeof error === 'object' && error !== null) {
    if (error.error) return String(error.error)
    if (error.message) return String(error.message)
  }
  return 'Operation failed'
}
