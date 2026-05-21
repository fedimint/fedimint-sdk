import { TransportClient } from '@fedimint/core'
import { FedimintWallet } from '@fedimint/core/testing'
import { expect, vi } from 'vitest'
import { walletTest } from './test/fixtures'

walletTest('get invite code from devimint', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  const inviteCode = await wallet.testing.getInviteCode()
  expect(inviteCode).toBeDefined()
})

walletTest('fund wallet with devimint', async ({ fundedWallet }) => {
  expect(fundedWallet).toBeDefined()
  const balance = await fundedWallet.balance.getBalance()
  expect(balance).toBeGreaterThan(0)
})

walletTest('joinFederation passes options to the transport', async () => {
  const sendSingleMessage = vi.fn().mockResolvedValue(undefined)
  const wallet = new FedimintWallet({
    sendSingleMessage,
    logger: {
      error: vi.fn(),
    },
  } as unknown as TransportClient)

  await expect(
    wallet.joinFederation('fed123...', {
      clientName: 'my-client-name',
      forceRecover: true,
    }),
  ).resolves.toBe(true)

  expect(sendSingleMessage).toHaveBeenCalledWith('join_federation', {
    invite_code: 'fed123...',
    client_name: 'my-client-name',
    force_recover: true,
  })
})

walletTest('joinFederation still accepts a client name string', async () => {
  const sendSingleMessage = vi.fn().mockResolvedValue(undefined)
  const wallet = new FedimintWallet({
    sendSingleMessage,
    logger: {
      error: vi.fn(),
    },
  } as unknown as TransportClient)

  await expect(
    wallet.joinFederation('fed123...', 'my-client-name'),
  ).resolves.toBe(true)

  expect(sendSingleMessage).toHaveBeenCalledWith('join_federation', {
    invite_code: 'fed123...',
    client_name: 'my-client-name',
    force_recover: false,
  })
})

walletTest(
  'Error on open & join if wallet is already open',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    // Test opening an already open wallet
    try {
      await wallet.open()
      expect.unreachable(
        'Opening a wallet should fail on an already open wallet',
      )
    } catch (error) {
      expect(error).toBeInstanceOf(Error)
      expect((error as Error).message).toBe(
        'The FedimintWallet is already open.',
      )
    }

    // Test joining federation on an already open wallet
    try {
      await wallet.joinFederation(wallet.testing.TESTING_INVITE)
      expect.unreachable('Joining a federation should fail on an open wallet')
    } catch (error) {
      expect(error).toBeInstanceOf(Error)
      expect((error as Error).message).toBe(
        'The FedimintWallet is already open. You can only call `joinFederation` on closed clients.',
      )
    }
  },
)

walletTest('getConfig', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  expect(wallet.isOpen()).toBe(true)
  const config = await wallet.federation.getConfig()
  expect(config).toBeDefined()
})

walletTest('empty getBalance', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  expect(wallet.isOpen()).toBe(true)
  await expect(wallet.waitForOpen()).resolves.toBeUndefined()
  await expect(wallet.balance.getBalance()).resolves.toEqual(0)
})

walletTest('previewFederation', async ({ walletDirector, unopenedWallet }) => {
  const preview = await walletDirector.previewFederation(
    unopenedWallet.testing.TESTING_INVITE,
  )
  expect(preview).toBeDefined()
  expect(preview.config).toBeDefined()
  expect(preview.federation_id).toBeDefined()
  expect(preview).toMatchObject({
    config: expect.any(Object),
    federation_id: expect.any(String),
  })
})
