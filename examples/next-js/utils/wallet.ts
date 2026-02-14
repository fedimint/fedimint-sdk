import { type FedimintWallet, WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

let wallet: FedimintWallet | undefined
let director: WalletDirector | undefined

if (typeof window !== 'undefined') {
  director = new WalletDirector(new WasmWorkerTransport() as unknown as any)
  director.setLogLevel('debug')
}

const getWallet = async () => {
  if (!director) {
    throw new Error('WalletDirector unavailable')
  }
  if (!wallet) {
    wallet = await director.createWallet()
  }
  return wallet
}

const getWalletSync = () => wallet

const initializeWallet = async () => {
  const currentWallet = await getWallet()
  try {
    if (!currentWallet.isOpen()) {
      await currentWallet.open()
    }
  } catch (error) {
    console.warn('Wallet open failed, continuing...', error)
  }
  return currentWallet
}

export { director, getWallet, getWalletSync, initializeWallet }
