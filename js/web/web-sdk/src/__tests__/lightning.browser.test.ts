import { describe, expect } from 'vitest'
import {
  ErrorCode,
  LightningRoute_Tags,
  LnReceiveState_Tags,
  LnSendState_Tags,
  OperationKind,
} from '@fedimint/web-sdk'
import { FUNDING, balanceBelow, faucet, sdkTest } from './fixtures'

// Two mainnet invoices, from the crate's own tests: one without an amount, one with.
const AMOUNTLESS_MAINNET =
  'lnbc1pj48ugqdq0dehjqctdda6kuaqpp5yg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3qsp5xvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxvenxves9qrsgqcqzyswm4efuu52zkzgrcc35fra9fmvj7s9ppxmej85s83hjkh7crcy9vqlradwalsmq40knf3552panjvlhjlrfazmvs86krxuaygut8v30sq0y0422'
const MAINNET =
  'lnbc25m1pvjluezpp5qqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqqqsyqcyq5rqwzqfqypqdq5vdhkven9v5sxyetpdeessp5zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zyg3zygs9q5sqqqqqqqqqqqqqqqpqsq67gye39hfg3zd8rgc80k32tvy9xk2xunwm5lzexnvpx6fd77en8qaq424dxgt56cag2dpt359k3ssyhetktkpqh24jqnjyw6uqd08sgptq44qu'

describe('lightning', () => {
  sdkTest(
    'receive issues an invoice and the operation follows the payment to Claimed',
    async ({ federation }) => {
      const lightning = (await federation.lightning())!
      const receive = await lightning.receive(FUNDING, 'web-sdk test')
      expect(receive.invoice).toMatch(/^lnbcrt/)
      expect(await receive.operation.id()).toMatch(/^[0-9a-f]{64}$/)
      let state = await receive.operation.state()
      expect([
        LnReceiveState_Tags.Created,
        LnReceiveState_Tags.WaitingForPayment,
      ]).toContain(state.tag)

      const updates = await receive.operation.updates()
      await faucet.pay(receive.invoice)
      while (state.tag !== LnReceiveState_Tags.Claimed) {
        const next = await updates.next()
        if (next === undefined) break
        expect([
          LnReceiveState_Tags.Created,
          LnReceiveState_Tags.WaitingForPayment,
          LnReceiveState_Tags.Funded,
          LnReceiveState_Tags.Claimed,
        ]).toContain(next.tag)
        state = next
      }
      expect(state.tag).toBe(LnReceiveState_Tags.Claimed)
      // Final: the subscription ends, and `awaitFinal` answers at once.
      expect(await updates.next()).toBeUndefined()
      expect((await receive.operation.awaitFinal()).tag).toBe(
        LnReceiveState_Tags.Claimed,
      )
      const details = await receive.operation.details()
      expect(details.invoice).toBe(receive.invoice)
      expect(await federation.balance()).toBeGreaterThan(0n)
    },
  )

  sdkTest(
    'refuses what cannot be received or quoted, naming why',
    async ({ federation }) => {
      const lightning = (await federation.lightning())!
      await expect(lightning.receive(0n, 'nothing')).rejects.toMatchObject({
        code: ErrorCode.InvalidInput,
      })
      await expect(
        lightning.receive(1_000n, 'x'.repeat(640)),
      ).rejects.toMatchObject({
        code: ErrorCode.InvalidInput,
      })
      await expect(lightning.quote(AMOUNTLESS_MAINNET)).rejects.toMatchObject({
        code: ErrorCode.AmountlessInvoice,
      })
      // The structured details cross too.
      const mismatch = await lightning.quote(MAINNET).catch((e) => e)
      expect(mismatch.code).toBe(ErrorCode.NetworkMismatch)
      expect(mismatch.details).toBeDefined()
      // A regtest invoice on an empty wallet: everything upstream works, the balance refuses.
      await expect(
        lightning.quote(await faucet.invoice(50_000n)),
      ).rejects.toMatchObject({
        code: ErrorCode.InsufficientBalance,
      })
    },
  )

  sdkTest(
    'quotes and pays an invoice from outside the federation',
    async ({ funded }) => {
      const { federation, balance } = funded
      const lightning = (await federation.lightning())!
      const invoice = await faucet.invoice(10_000n)

      const quote = await lightning.quote(invoice)
      expect(await quote.invoiceAmount()).toBe(10_000n)
      const fee = await quote.fee()
      expect(fee).toBeGreaterThanOrEqual(0n)
      expect(await quote.total()).toBe(10_000n + fee)
      const breakdown = await quote.feeBreakdown()
      for (const part of [
        breakdown.gateway,
        breakdown.lightningModule,
        breakdown.primaryModule,
        breakdown.dust,
      ]) {
        expect(part).toBeTypeOf('bigint')
        expect(part).toBeGreaterThanOrEqual(0n)
      }
      expect((await quote.route()).tag).toBe(LightningRoute_Tags.Gateway)
      expect(await quote.expiresAt()).toBeGreaterThan(BigInt(Date.now()))

      const second = await lightning.quote(invoice)
      const operation = await lightning.send(quote)
      const started = await operation.state()
      expect([
        LnSendState_Tags.Created,
        LnSendState_Tags.Funded,
        LnSendState_Tags.Success,
      ]).toContain(started.tag)
      expect((await operation.awaitFinal()).tag).toBe(LnSendState_Tags.Success)
      const after = await balanceBelow(federation, balance)
      expect(balance - after).toBeGreaterThanOrEqual(10_000n)

      // The invoice is paid; the other quote for it is refused as already executed.
      await expect(lightning.send(second)).rejects.toMatchObject({
        name: 'SdkError',
        code: ErrorCode.QuoteExpired,
      })

      const kinds = (await federation.activity(undefined, 10)).items.map(
        (item) => item.kind,
      )
      expect(kinds).toEqual(
        expect.arrayContaining([OperationKind.LnSend, OperationKind.LnReceive]),
      )
    },
  )
})
