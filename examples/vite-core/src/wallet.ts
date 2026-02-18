import { type FedimintWallet, WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
let wallet: FedimintWallet | undefined

const getWallet = async () => {
  if (!wallet) {
    console.log('Creating wallet...')
    wallet = await director.createWallet()
  }
  return wallet
}

const getWalletSync = () => wallet

director.setLogLevel('debug')

export { wallet, director, getWallet, getWalletSync }
