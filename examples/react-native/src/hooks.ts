import { useState, useCallback, useEffect } from 'react'
import { wallet } from './wallet'

// Hook to monitor and control whether the Fedimint wallet connection is open
export const useIsOpen = () => {
  const [open, setIsOpen] = useState(false)

  // Synchronize our local React state with the core wallet state
  const checkIsOpen = useCallback(() => {
    if (wallet && open !== wallet.isOpen()) {
      setIsOpen(wallet.isOpen())
    }
  }, [open])

  useEffect(() => {
    // Attempt to open the wallet automatically when the hooks mounts
    const tryOpen = async () => {
      try {
        if (wallet && !wallet.isOpen()) {
          console.log('Attempting to open wallet on startup...')
          await wallet.open()
        }
      } catch (e) {
        console.log('Wallet could not be opened on startup (might not be joined): ', e)
      }
    }

    // Immediately verify the state
    checkIsOpen()

    // Background listener: Wait patiently until the wallet successfully opens then update UI
    if (wallet) {
      wallet.waitForOpen().then(() => checkIsOpen()).catch(console.error)
    }

    tryOpen()
  }, [checkIsOpen])

  return { open, checkIsOpen }
}

// Hook to subscribe to real-time wallet balance changes
export const useBalance = (checkIsOpen: () => void) => {
  const [balance, setBalance] = useState(0)

  useEffect(() => {
    const unsubscribe = wallet?.balance.subscribeBalance((bal: number) => {
      checkIsOpen() // Make sure we confirm it's open if we somehow receive a balance
      setBalance(bal)
    })

    // Clean up subscription when component unmounts
    return () => {
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
