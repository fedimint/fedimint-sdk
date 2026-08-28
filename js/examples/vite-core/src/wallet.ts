import { type FedimintWallet, WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
let wallet: FedimintWallet | undefined
director.createWallet().then((_wallet) => {
  console.log('Creating wallet...')
  _wallet.open()
  wallet = _wallet

  // Expose the wallet to the global window object for testing
  // @ts-ignore
  globalThis.wallet = wallet
  // @ts-ignore
  globalThis.director = director
})

director.setLogLevel('debug')

export { wallet, director }
