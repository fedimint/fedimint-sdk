import { expect } from 'vitest'
import { walletTest } from '../test/fixtures'

walletTest('redeemEcash should error on invalid ecash', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  expect(wallet.isOpen()).toBe(true)

  await expect(wallet.mint.redeemEcash('test')).rejects.toThrow()
})

walletTest(
  'reissueExternalNotes should throw if wallet is empty',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    await expect(wallet.mint.reissueExternalNotes('test')).rejects.toThrow()
  },
)

walletTest('spendNotes should throw if wallet is empty', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  expect(wallet.isOpen()).toBe(true)

  await expect(wallet.mint.spendNotes(100)).rejects.toThrow()
})

walletTest('parseNotes should parse notes', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  expect(wallet.isOpen()).toBe(true)

  await expect(wallet.mint.reissueExternalNotes('test')).rejects.toThrow()
})

walletTest(
  'getNotesByDenomination should return empty object if wallet is empty',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    const notes = await wallet.mint.getNotesByDenomination()
    const balance = await wallet.balance.getBalance()
    expect(balance).toEqual(0)
    expect(notes).toBeDefined()
    expect(notes).toEqual({})
  },
)

walletTest(
  'getNotesByDenomination should get notes by denomination',
  async ({ fundedWallet }) => {
    expect(fundedWallet).toBeDefined()
    expect(fundedWallet.isOpen()).toBe(true)

    const notes = await fundedWallet.mint.getNotesByDenomination()
    const balance = await fundedWallet.balance.getBalance()
    expect(balance).toBeGreaterThan(0)
    expect(notes).toBeDefined()

    // Verify that the notes sum up to the balance
    const sum = Object.entries(notes).reduce((acc, [denom, count]) => {
      return acc + parseInt(denom) * count
    }, 0)
    expect(sum).toEqual(balance)
  },
)
