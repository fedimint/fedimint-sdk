import { WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
director.setLogLevel('debug')

let wallet
const walletReady = director.createWallet().then((w) => {
  wallet = w
  return w
})

export { director, wallet, walletReady }
