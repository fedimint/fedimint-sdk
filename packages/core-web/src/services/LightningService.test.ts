import { expect } from 'vitest'
import { keyPair } from '../test/crypto'
import { walletTest } from '../test/fixtures'

walletTest(
  'createInvoice should create a bolt11 invoice',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    const counterBefore = wallet.testing.getRequestCounter()
    const invoice = await wallet.lightning.createInvoice(100, 'test')
    expect(invoice).toBeDefined()
    expect(invoice).toMatchObject({
      invoice: expect.any(String),
      operation_id: expect.any(String),
    })
    // 3 requests were made, one for the invoice, one for refreshing the
    // gateway cache, one for getting the gateway info
    expect(wallet.testing.getRequestCounter()).toBe(counterBefore + 3)

    // Test with expiry time
    await expect(
      wallet.lightning.createInvoice(100, 'test', 1000),
    ).resolves.toBeDefined()
  },
)

walletTest('createInvoice with expiry', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  expect(wallet.isOpen()).toBe(true)

  const invoice = await wallet.lightning.createInvoice(100, 'test', 1000)
  expect(invoice).toBeDefined()
  expect(invoice).toMatchObject({
    invoice: expect.any(String),
    operation_id: expect.any(String),
  })
})

walletTest(
  'listGateways should return a list of gateways',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    const counterBefore = wallet.testing.getRequestCounter()
    const gateways = await wallet.lightning.listGateways()
    expect(wallet.testing.getRequestCounter()).toBe(counterBefore + 1)
    expect(gateways).toBeDefined()
    expect(gateways).toMatchObject(expect.any(Array))
  },
)

walletTest(
  'updateGatewayCache should update the gateway cache',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    const counterBefore = wallet.testing.getRequestCounter()
    await expect(wallet.lightning.updateGatewayCache()).resolves.toBeDefined()
    expect(wallet.testing.getRequestCounter()).toBe(counterBefore + 1)
  },
)

walletTest('getGateway should return a gateway', async ({ wallet }) => {
  expect(wallet).toBeDefined()
  expect(wallet.isOpen()).toBe(true)

  const counterBefore = wallet.testing.getRequestCounter()
  const gateway = await wallet.lightning.getGateway()
  expect(wallet.testing.getRequestCounter()).toBe(counterBefore + 1)
  expect(gateway).toMatchObject({
    api: expect.any(String),
    fees: expect.any(Object),
    gateway_id: expect.any(String),
    gateway_redeem_key: expect.any(String),
    lightning_alias: expect.any(String),
    mint_channel_id: expect.any(Number),
    node_pub_key: expect.any(String),
    route_hints: expect.any(Array),
  })
})

walletTest(
  'payInvoice should throw on insufficient funds',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    const invoice = await wallet.lightning.createInvoice(100, 'test')
    expect(invoice).toBeDefined()
    expect(invoice).toMatchObject({
      invoice: expect.any(String),
      operation_id: expect.any(String),
    })

    const counterBefore = wallet.testing.getRequestCounter()
    // Insufficient funds
    try {
      await wallet.lightning.payInvoice(invoice.invoice)
      expect.unreachable('Should throw error')
    } catch (error) {
      expect(error).toBeDefined()
    }
    // 3 requests were made, one for paying the invoice, one for refreshing the
    // gateway cache, one for getting the gateway info
    expect(wallet.testing.getRequestCounter()).toBe(counterBefore + 3)
  },
)

walletTest(
  'payInvoice should pay a bolt11 invoice',
  { timeout: 20_000 },
  async ({ fundedWallet }) => {
    expect(fundedWallet).toBeDefined()
    expect(fundedWallet.isOpen()).toBe(true)
    const initialBalance = await fundedWallet.balance.getBalance()
    expect(initialBalance).toBeGreaterThan(0)
    const externalInvoice = await fundedWallet.testing.createFaucetInvoice(1)
    const gatewayInfo = await fundedWallet.testing.getFaucetGatewayInfo()
    const payment = await fundedWallet.lightning.payInvoice(
      externalInvoice,
      gatewayInfo,
    )
    expect(payment).toMatchObject({
      contract_id: expect.any(String),
      fee: expect.any(Number),
      payment_type: expect.any(Object),
    })
    const finalBalance = await fundedWallet.balance.getBalance()
    expect(finalBalance).toBeLessThan(initialBalance)
  },
)

walletTest(
  'createInvoiceTweaked should create a bolt11 invoice with a tweaked public key',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    // Make an ephemeral key pair
    const { publicKey, secretKey } = keyPair()
    const tweak = 1

    // Create an invoice paying to the tweaked public key
    const invoice = await wallet.lightning.createInvoiceTweaked(
      1000,
      'test tweaked',
      publicKey,
      tweak,
    )
    expect(invoice).toBeDefined()
    expect(invoice).toMatchObject({
      invoice: expect.any(String),
      operation_id: expect.any(String),
    })
  },
)

walletTest(
  'scanReceivesForTweaks should return the operation id',
  async ({ wallet }) => {
    expect(wallet).toBeDefined()
    expect(wallet.isOpen()).toBe(true)

    // Make an ephemeral key pair
    const { publicKey, secretKey } = keyPair()
    const tweak = 1

    const gatewayInfo = await wallet.testing.getFaucetGatewayInfo()

    // Create an invoice paying to the tweaked public key
    const invoice = await wallet.lightning.createInvoiceTweaked(
      1000,
      'test tweaked',
      publicKey,
      tweak,
      undefined,
      gatewayInfo,
    )
    await expect(
      wallet.testing.payFaucetInvoice(invoice.invoice),
    ).resolves.toBeDefined()

    // Scan for the receive
    const operationIds = await wallet.lightning.scanReceivesForTweaks(
      secretKey,
      [tweak],
      {},
    )
    expect(operationIds).toBeDefined()
    expect(operationIds).toHaveLength(1)

    // Subscribe to claiming the receive
    const subscription = await wallet.lightning.subscribeLnClaim(
      operationIds[0],
      (state) => {
        expect(state).toBeDefined()
        expect(state).toMatchObject({
          state: 'claimed',
        })
      },
    )
    expect(subscription).toBeDefined()
  },
)
