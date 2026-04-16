import WalletDirector from '@fedimint/react-native'
import type { FedimintWallet } from '@fedimint/react-native'
import { Paths } from 'expo-file-system'

// Paths.document.uri is a file:// URI; the Rust FFI layer needs a raw path
const dbPath = Paths.document.uri.replace(/^file:\/\//, '') + 'fedimint_db'

const director = new WalletDirector(dbPath)
let wallet: FedimintWallet | undefined

const walletReady: Promise<FedimintWallet> = director
  .createWallet()
  .then((_wallet) => {
    console.log('Creating wallet...')
    wallet = _wallet
    return _wallet
  })

director.setLogLevel('debug')

export { wallet, director, walletReady }
