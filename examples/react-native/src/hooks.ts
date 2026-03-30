import { useState, useCallback, useEffect } from 'react'
import { wallet } from './wallet'

export const useIsOpen = () => {
  const [open, setIsOpen] = useState(false)

  const checkIsOpen = useCallback(() => {
    if (wallet && open !== wallet.isOpen()) {
      setIsOpen(wallet.isOpen())
    }
  }, [open])

  useEffect(() => {
    const tryOpen = async () => {
      try {
        if (wallet && !wallet.isOpen()) {
          console.log('Attempting to open wallet on startup...')
          await wallet.open()
          checkIsOpen()
        }
      } catch (e) {
        console.log('Wallet could not be opened on startup (might not be joined): ', e)
      }
    }
    tryOpen()
    checkIsOpen()
  }, [checkIsOpen])

  return { open, checkIsOpen }
}

export const useBalance = (checkIsOpen: () => void) => {
  const [balance, setBalance] = useState(0)

  useEffect(() => {
    const unsubscribe = wallet?.balance.subscribeBalance((bal) => {
      checkIsOpen()
      setBalance(bal)
    })

    return () => {
      unsubscribe?.()
    }
  }, [checkIsOpen])

  return balance
}
