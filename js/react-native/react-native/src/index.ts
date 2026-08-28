import { ReactNativeTransport } from './ReactNativeTransport'
import {
  WalletDirector as BaseWalletDirector,
  TransportClient,
} from '@fedimint/core'
import type { FedimintWallet } from '@fedimint/core'

/**
 * WalletDirector for React Native.
 * Automatically uses ReactNativeTransport under the hood.
 *
 * @example
 * ```typescript
 * import WalletDirector from '@fedimint/react-native';
 * import RNFS from 'react-native-fs';
 *
 * const dbPath = `${RNFS.DocumentDirectoryPath}/fedimint_db`;
 * const director = new WalletDirector(dbPath);
 * ```
 */
export class WalletDirector extends BaseWalletDirector {
  constructor(dbPath: string, lazy: boolean = false) {
    const transport = new ReactNativeTransport(dbPath)
    super(transport, dbPath, lazy)
  }
}

// Default export for simple usage: import WalletDirector from '@fedimint/react-native'
export default WalletDirector

// Named exports for advanced users
export { ReactNativeTransport, TransportClient }
export type { FedimintWallet }
