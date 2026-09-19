/* eslint-disable no-console */
import { AppiumTestBase } from '../configs/appium/AppiumTestBase'
import { FUNDING_MSATS } from '../fixtures/fundWallet'
import { receiveOverLightning } from '../flows/wallet'

// Lightning in, over a real federation and a real gateway: the app issues an
// invoice, devimint's faucet pays it, and the balance the SDK reports goes up.
// The wasm suite's `fundWallet` is the same path; here it crosses the FFI
// boundary and runs on a device, which is what this suite exists to prove.
//
// Leaves the wallet funded, so MintService can spend without funding again.
export class LightningService extends AppiumTestBase {
  static prerequisites: readonly string[] = ['walletOpen', 'joinedFederation']
  static produces: readonly string[] = [
    'walletOpen',
    'joinedFederation',
    'funded',
  ]

  async execute(): Promise<void> {
    console.log('Starting LightningService test')

    const balance = await receiveOverLightning(this, FUNDING_MSATS)

    if (balance <= 0) {
      throw new Error(`Receive settled but the balance is ${balance} msat`)
    }
    // The gateway's fee comes out of what arrives, and how much that is, is
    // the Rust integration tests' business (rust/fedimint-sdk), not this
    // suite's. What a device proves is that the money arrived and the live
    // balance subscription delivered it — so bound it rather than pin it.
    if (balance > FUNDING_MSATS) {
      throw new Error(
        `Received ${balance} msat for a ${FUNDING_MSATS} msat invoice`,
      )
    }

    console.log(`LightningService test passed with ${balance} msat received`)
  }
}
