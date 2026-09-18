// Node-side port of js/web/integration-tests/src/test/TestingService.ts's
// faucet HTTP calls — that version reads `import.meta.env.FAUCET` (Vite-only),
// this one reads `process.env.FAUCET` since the Appium runner is a plain
// ts-node CLI process. Same devimint faucet API underneath
// (see scripts/setup_test_shell.sh), so any test here can join the same
// local regtest federation the WASM integration tests use.
//
// Not exercised by the v1 MnemonicService smoke test (deliberately
// federation-free) — this exists for the next test that needs one, e.g. a
// FederationService.test.ts that joins via invite code and pays an invoice.
export class FaucetClient {
  private readonly baseUrl: string

  constructor(baseUrl = process.env.FAUCET || 'http://localhost:15243') {
    this.baseUrl = baseUrl
  }

  async getInviteCode(): Promise<string> {
    const res = await fetch(`${this.baseUrl}/connect-string`)
    const text = await res.text()
    if (!res.ok) throw new Error(`Failed to get invite code: ${text}`)
    return text
  }

  async payInvoice(invoice: string): Promise<string> {
    const res = await fetch(`${this.baseUrl}/pay`, {
      method: 'POST',
      body: invoice,
    })
    const text = await res.text()
    if (!res.ok) throw new Error(`Failed to pay faucet invoice: ${text}`)
    return text
  }

  async createInvoice(amountSats: number): Promise<string> {
    const res = await fetch(`${this.baseUrl}/invoice`, {
      method: 'POST',
      body: amountSats.toString(),
    })
    const text = await res.text()
    if (!res.ok) throw new Error(`Failed to generate faucet invoice: ${text}`)
    return text
  }
}
