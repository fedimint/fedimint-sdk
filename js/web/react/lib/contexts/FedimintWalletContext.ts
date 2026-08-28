import { WalletDirector, FedimintWallet, Transport } from '@fedimint/core'
import {
  createContext,
  createElement,
  useEffect,
  useMemo,
  useState,
} from 'react'

let wallet: FedimintWallet
let walletDirector: WalletDirector

type FedimintWalletConfig = {
  lazy?: boolean
  debug?: boolean
  transport: Transport
}

export type WalletStatus = 'open' | 'closed' | 'opening'

export const setupFedimintWallet = (config: FedimintWalletConfig) => {
  walletDirector = new WalletDirector(config.transport)
  walletDirector.createWallet().then((w) => {
    wallet = w
  })
  if (config.debug) {
    walletDirector.setLogLevel('debug')
  }
}

export const FedimintWalletContext = createContext<
  | {
      wallet: FedimintWallet
      walletStatus: WalletStatus
      setWalletStatus: (status: WalletStatus) => void
    }
  | undefined
>(undefined)

export type FedimintWalletProviderProps = {}

export const FedimintWalletProvider = (
  parameters: React.PropsWithChildren<FedimintWalletProviderProps>,
) => {
  const [walletStatus, setWalletStatus] = useState<WalletStatus>('closed')
  const { children } = parameters

  if (!wallet)
    throw new Error(
      'You must call setupFedimintWallet() first. See the getting started guide.',
    )

  const contextValue = useMemo(
    () => ({
      wallet,
      walletStatus,
      setWalletStatus,
    }),
    [walletStatus],
  )

  useEffect(() => {
    wallet.waitForOpen().then(() => {
      setWalletStatus('open')
    })
  }, [wallet])

  return createElement(
    FedimintWalletContext.Provider,
    { value: contextValue },
    children,
  )
}
