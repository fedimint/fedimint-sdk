import { FedimintWallet, Transport, WalletDirector } from '@fedimint/core'

let wallet: FedimintWallet | undefined

let walletDirector: WalletDirector

export const initFedimintReact = (
  lazy: boolean = false,
  transport: Transport,
) => {
  walletDirector = new WalletDirector(transport)
  if (!lazy) {
    walletDirector.createWallet().then((w) => {
      wallet = w
    })
  }

  return {}
}
