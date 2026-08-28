import { expect } from 'vitest'
import { walletTest } from '../test/fixtures'
import { TxOutputSummary, WalletSummary } from '@fedimint/core'

walletTest(
  'getWalletSummary should return empty object if wallet is empty',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    const balance = await wallet.balance.getBalance()
    expect(balance).toEqual(0)

    const summary = await wallet.wallet.getWalletSummary()
    const expectedSummary = {
      spendable_utxos: expect.any(Array<TxOutputSummary>),
      unsigned_peg_out_txos: expect.any(Array<TxOutputSummary>),
      unsigned_change_utxos: expect.any(Array<TxOutputSummary>),
      unconfirmed_peg_out_txos: expect.any(Array<TxOutputSummary>),
      unconfirmed_change_utxos: expect.any(Array<TxOutputSummary>),
    } satisfies WalletSummary
    expect(summary).toEqual(expect.objectContaining(expectedSummary))
  },
)

walletTest(
  'generateAddress should always return an address',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)
    // Works around an upstream crash: peg_in right after joining races the
    // wallet module's background DB writes and panics on the resulting
    // WriteConflict (https://github.com/fedimint/fedimint/issues/9046). The
    // delay shrinks the window but cannot close it; remove once the fix lands
    // in the pinned wasm.
    await new Promise((resolve) => setTimeout(resolve, 100))
    const response = await wallet.wallet.generateAddress()

    expect(response, 'generateAddress').toEqual({
      deposit_address: expect.any(String),
      operation_id: expect.any(String),
    })
  },
)

walletTest(
  'sendOnchain should return an operation ID after sending funds',
  async ({ fundedWalletBeefy }) => {
    expect(fundedWalletBeefy).toBeDefined()
    expect(fundedWalletBeefy.isOpen()).toBe(true)

    const amountSat = 100
    const address =
      'bcrt1qphk8q2v8he2autevdcefnnwjl4yc2hm74uuvhaa6nhrnkd3gfrwq6mnr76'

    const response = await fundedWalletBeefy.wallet.sendOnchain(
      amountSat,
      address,
    )

    expect(response, 'send onchain').toEqual({
      operation_id: expect.any(String),
    })
  },
)
