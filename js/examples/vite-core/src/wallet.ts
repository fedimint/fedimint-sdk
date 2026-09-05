import { type FedimintWallet, WalletDirector } from '@fedimint/core'
import { WasmWorkerTransport } from '@fedimint/transport-web'

const director = new WalletDirector(new WasmWorkerTransport())
director.setLogLevel('debug')

/**
 * Lazily initializes and returns the wallet singleton.
 * Every caller awaits the same promise — no race conditions,
 * no mutable module-level variable to accidentally read before init.
 */
let _walletPromise: Promise<FedimintWallet> | null = null

function getWallet(): Promise<FedimintWallet> {
  if (!_walletPromise) {
    _walletPromise = director.createWallet().then((w) => {
      console.log('Wallet created, waiting for onboarding...')
      // Expose for debugging only
      // @ts-ignore
      globalThis.wallet = w
      // @ts-ignore
      globalThis.director = director
      return w
    })
  }
  return _walletPromise
}

export { director, getWallet }
