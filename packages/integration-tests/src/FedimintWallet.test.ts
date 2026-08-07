import { TransportClient, WalletDirector, type Transport } from '@fedimint/core'
import { FedimintWallet } from '@fedimint/core/testing'
import { expect, vi } from 'vitest'
import { walletTest } from './test/fixtures'

function sumNoteCounts(noteCounts: Record<string, number>) {
  return Object.entries(noteCounts).reduce((sum, [denomination, count]) => {
    return sum + Number(denomination) * count
  }, 0)
}

class StaticResponseTransport {
  logger = {
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }

  private messageHandler: Parameters<Transport['setMessageHandler']>[0] =
    () => {}

  constructor(private readonly response: unknown) {}

  setMessageHandler(handler: Parameters<Transport['setMessageHandler']>[0]) {
    this.messageHandler = handler
  }

  setErrorHandler() {}

  postMessage(message: Parameters<Transport['postMessage']>[0]) {
    this.messageHandler({
      type: 'data',
      request_id: message.requestId,
      data: message.type === 'init' ? true : this.response,
    })
  }
}

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

walletTest(
  'parseOobNotes should parse locally spent notes via wasm',
  async ({ walletDirector, fundedWallet }) => {
    expect(fundedWallet).toBeDefined()
    expect(fundedWallet.isOpen()).toBe(true)

    const spendAmount = 4096
    const federationId = await fundedWallet.federation.getFederationId()
    const inviteCode = await fundedWallet.federation.getInviteCode(0)

    if (!inviteCode) {
      throw new Error('Expected federation invite code to be available')
    }

    const { operation_id: spendOperationId, notes } =
      await fundedWallet.mint.spendNotes(spendAmount, 3600, true)
    expect(spendOperationId).toBeTypeOf('string')

    const parsed = await walletDirector.parseOobNotes(notes)
    expect(parsed).toMatchObject({
      total_amount: spendAmount,
      federation_id_prefix: federationId.slice(0, 8),
      federation_id: federationId,
      invite_code: inviteCode,
    })
    expect(sumNoteCounts(parsed.note_counts)).toEqual(spendAmount)
  },
  30000,
)

walletTest.each([
  { name: 'non-array values', prefix: '00010203' },
  { name: 'too few bytes', prefix: [0, 1, 2] },
  { name: 'too many bytes', prefix: [0, 1, 2, 3, 4] },
  { name: 'negative bytes', prefix: [0, 1, 2, -1] },
  { name: 'bytes above 255', prefix: [0, 1, 2, 256] },
  { name: 'non-integer bytes', prefix: [0, 1, 2, 3.5] },
])(
  'parseOobNotes rejects malformed federation ID prefixes: $name',
  async ({ prefix }) => {
    const director = new WalletDirector(
      new StaticResponseTransport({
        total_amount: 1000,
        federation_id_prefix: prefix,
        federation_id: null,
        invite_code: null,
        note_counts: { '1000': 1 },
      }) as unknown as Transport,
      undefined,
      true,
    )

    await expect(director.parseOobNotes('notes')).rejects.toThrow(
      'Invalid parse_oob_notes response: federation_id_prefix must contain four bytes',
    )
  },
)
