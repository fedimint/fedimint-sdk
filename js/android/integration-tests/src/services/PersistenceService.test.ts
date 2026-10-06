/* eslint-disable no-console */
import { AppiumTestBase } from '../configs/appium/AppiumTestBase'
import {
  readBalanceMsats,
  readFederationId,
  readSeedWords,
  restartAndReopen,
  waitForBalance,
} from '../flows/wallet'

// The app's storage outliving the app: kill the process without clearing its
// data, open the SDK over the same directory, and get back the same seed, the
// same federation and the same balance — the state a user expects after
// closing and reopening a wallet.
//
// Opening the SDK again also re-takes the storage lock the killed process
// held, which a lock leaked across the restart would refuse with
// STORAGE_IN_USE.
export class PersistenceService extends AppiumTestBase {
  static prerequisites: readonly string[] = [
    'walletOpen',
    'joinedFederation',
    'funded',
  ]
  static produces: readonly string[] = [
    'walletOpen',
    'joinedFederation',
    'funded',
  ]

  async execute(): Promise<void> {
    console.log('Starting PersistenceService test')

    const seed = await readSeedWords(this)
    const federationId = await readFederationId(this)
    const balance = await readBalanceMsats(this)
    if (balance <= 0) {
      throw new Error(
        `Expected a funded wallet to persist, holds ${balance} msat`,
      )
    }
    console.log(`Before: federation ${federationId}, ${balance} msat`)

    const reopened = await restartAndReopen(this)
    if (!reopened.includes('reattached to')) {
      throw new Error(
        `Reopening the same storage should reattach the joined federation: "${reopened}"`,
      )
    }

    const seedAfter = await readSeedWords(this)
    if (seedAfter.join(' ') !== seed.join(' ')) {
      throw new Error('The seed changed across a restart')
    }
    const federationAfter = await readFederationId(this)
    if (federationAfter !== federationId) {
      throw new Error(
        `Reattached to ${federationAfter}, expected ${federationId}`,
      )
    }
    // The balance line starts at "…" and fills from the reattached handle.
    await waitForBalance(this, balance)

    console.log('PersistenceService test passed')
  }
}
