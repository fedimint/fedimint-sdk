import { expect } from 'vitest'
import { walletTest } from '../test/fixtures'

walletTest('getBalance should be initially zero', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  expect(wallet.isOpen()).toBe(true)
  const beforeGetBalance = wallet.testing.getRequestCounter()
  await expect(wallet.balance.getBalance()).resolves.toEqual(0)
  expect(wallet.testing.getRequestCounter()).toBe(beforeGetBalance + 1)
})

walletTest('subscribe balance', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  expect(wallet.isOpen()).toBe(true)

  const counterBefore = wallet.testing.getRequestCounter()
  const callbacksBefore = wallet.testing.getRequestCallbackMap().size
  const unsubscribe = await wallet.balance.subscribeBalance((balance) => {
    expect(balance).toEqual(0)
  })
  expect(wallet.testing.getRequestCounter()).toBe(counterBefore + 1)
  expect(wallet.testing.getRequestCallbackMap().size).toBe(callbacksBefore + 1)

  await expect(wallet.balance.getBalance()).resolves.toEqual(0)
  expect(unsubscribe()).toBeUndefined()
})
