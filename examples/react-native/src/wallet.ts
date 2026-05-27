// @ts-ignore
import WalletDirector from '@fedimint/react-native'
// @ts-ignore
import type { FedimintWallet } from '@fedimint/react-native'
import RNFS from 'react-native-fs'

const dbPath = `${RNFS.DocumentDirectoryPath}/fedimint_db`

const director = new WalletDirector(dbPath)
let wallet: FedimintWallet | undefined

director.createWallet().then((_wallet: any) => {
  console.log('Creating wallet...')
  wallet = _wallet
})

director.setLogLevel('debug')

export { wallet, director }