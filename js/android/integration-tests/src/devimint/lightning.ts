/* eslint-disable no-console */
import { createHash } from 'crypto'

import { runCli, sleep } from './env'

// The far side of a lightning payment. The faucet issues its invoices from
// devimint's LDK gateway node (devimint/src/faucet.rs), so whether one was
// paid is that node's to say: `gateway-cli lightning get-invoice` is how
// devimint itself checks (`Gatewayd::wait_bolt11_invoice`).

/** The payment hash an invoice commits to, decoded by devimint's LND. */
export function paymentHash(invoice: string): string {
  const decoded = JSON.parse(runCli('FM_LNCLI', ['decodepayreq', invoice])) as {
    payment_hash?: string
  }
  if (!decoded.payment_hash || !/^[0-9a-f]{64}$/.test(decoded.payment_hash)) {
    throw new Error(
      `No payment hash in the decoded invoice: ${JSON.stringify(decoded)}`,
    )
  }
  return decoded.payment_hash
}

export function sha256Hex(hex: string): string {
  return createHash('sha256').update(Buffer.from(hex, 'hex')).digest('hex')
}

/**
 * Waits for the faucet's LDK node to report the invoice behind `hash` as
 * paid. The proof that the SDK's send reached the payee, independent of
 * anything the SDK itself reports.
 */
export async function waitForLdkInvoicePaid(
  hash: string,
  timeout: number,
): Promise<void> {
  const deadline = Date.now() + timeout
  let last = ''
  while (Date.now() < deadline) {
    last = runCli('FM_GWCLI_LDK', [
      'lightning',
      'get-invoice',
      '--payment-hash',
      hash,
    ])
    const status = findStatus(last ? JSON.parse(last) : null)
    if (status === 'Succeeded') return
    if (status === 'Failed') {
      throw new Error(
        `The payee reports the invoice ${hash} as failed: ${last}`,
      )
    }
    await sleep(1000)
  }
  throw new Error(
    `The payee never reported the invoice ${hash} as paid within ${timeout}ms — last: ${last}`,
  )
}

/**
 * The `status` of a `get-invoice` answer. The CLI prints an
 * `Option<GetInvoiceResponse>` inside its own output enum, so the record may
 * be bare, wrapped in one object layer, or `null` for an invoice the node does
 * not know (yet) — all three are read here, the last as "not paid".
 */
function findStatus(value: unknown): unknown {
  if (value === null || typeof value !== 'object') return undefined
  const record = value as Record<string, unknown>
  if ('status' in record) return record.status
  for (const inner of Object.values(record)) {
    const status = findStatus(inner)
    if (status !== undefined) return status
  }
  return undefined
}
