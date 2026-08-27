import { useCallback, useContext } from 'react'
import { FedimintWalletContext } from '../contexts/FedimintWalletContext'

export const useOpenWallet = () => {
  const value = useContext(FedimintWalletContext)

  if (!value) {
    throw new Error(
      'useOpenWallet must be used within a FedimintWalletProvider',
    )
  }

  const { wallet, walletStatus, setWalletStatus } = value

  const openWallet = useCallback(() => {
    if (walletStatus === 'open') return

    setWalletStatus('opening')
    wallet
      .open()
      .then(() => {
        setWalletStatus('open')
      })
      .catch(() => {
        setWalletStatus('closed')
      })
  }, [wallet])

  const joinFederation = useCallback(
    async (invite: string) => {
      if (walletStatus === 'open') return

      setWalletStatus('opening')

      await wallet.joinFederation(invite).then((res) => {
        setWalletStatus(res ? 'open' : 'closed')
      })
    },
    [wallet],
  )

  return { walletStatus, openWallet, joinFederation }
}
