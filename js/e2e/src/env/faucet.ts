// HTTP calls to devimint's faucet from Node. The URL comes from
// `process.env.FAUCET` since the runner is a plain ts-node CLI
// process. The faucet belongs to the local regtest federation that
// scripts/setup_test_shell.sh stands up, so any test here can join that
// federation.
//
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

  /** An invoice from devimint's LDK gateway node — a payee outside the
   * federation. The faucet reads the amount as millisatoshis. */
  async createInvoice(amountMsats: number): Promise<string> {
    const res = await fetch(`${this.baseUrl}/invoice`, {
      method: 'POST',
      body: amountMsats.toString(),
    })
    const text = await res.text()
    if (!res.ok) throw new Error(`Failed to generate faucet invoice: ${text}`)
    return text
  }
}
