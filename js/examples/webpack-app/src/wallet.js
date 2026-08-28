import { FedimintWallet } from '@fedimint/core'

const wallet = new FedimintWallet()

wallet.setLogLevel('debug')
wallet.open()

export { wallet }
