import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
director.setLogLevel('debug')

let wallet

const getWallet = async () => {
  if (!wallet) {
    wallet = await director.createWallet()
  }
  return wallet
}

export { director, getWallet }
