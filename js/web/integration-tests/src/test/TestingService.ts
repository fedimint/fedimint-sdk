import { TransportClient } from '@fedimint/core'
import { FedimintWallet } from '@fedimint/core/testing'

export const TESTING_INVITE =
  'fed11qgqrsdnhwden5te0v9cxjtt4dekxzamxw4kz6mmjvvkhydted9ukg6r9xfsnx7th0fhn26tf093juamwv4u8gtnpwpcz7qqpyz0e327ua8geceutfrcaezwt22mk6s2rdy09kg72jrcmncng2gn0kp2m5sk'

// This is a testing service that allows for inspecting the internals
// of the TransportClient. It is not intended for use in production.
export class TestingService {
  public TESTING_INVITE: string
  constructor(
    private client: TransportClient,
    private lightning: FedimintWallet['lightning'],
  ) {
    // Solo Mint on mutinynet
    this.TESTING_INVITE = TESTING_INVITE
  }

  getRequestCounter() {
    return this.client._getRequestCounter()
  }

  getRequestCallbackMap() {
    return this.client._getRequestCallbackMap()
  }

  async getInviteCode() {
    const res = await fetch(`${import.meta.env.FAUCET}/connect-string`)
    const text = await res.text()
    if (res.ok) {
      return text
    } else {
      throw new Error(`Failed to get invite code: ${text}`)
    }
  }

  private async getFaucetGatewayApi() {
    const res = await fetch(`${import.meta.env.FAUCET}/gateway-api`)
    if (res.ok) {
      return await res.text()
    } else {
      throw new Error(`Failed to get gateway: ${await res.text()}`)
    }
  }

  async getFaucetGatewayInfo() {
    await this.lightning.updateGatewayCache()
    const gateways = await this.lightning.listGateways()
    const api = await this.getFaucetGatewayApi()
    const gateway = gateways.find((g) => g.info.api === api)
    if (!gateway) {
      throw new Error(`Gateway not found: ${api}`)
    }
    return gateway.info
  }

  async payFaucetInvoice(invoice: string) {
    const res = await fetch(`${import.meta.env.FAUCET}/pay`, {
      method: 'POST',
      body: invoice,
    })
    if (res.ok) {
      return await res.text()
    } else {
      throw new Error(`Failed to pay faucet invoice: ${await res.text()}`)
    }
  }

  async createFaucetInvoice(amount: number) {
    const res = await fetch(`${import.meta.env.FAUCET}/invoice`, {
      method: 'POST',
      body: amount.toString(),
    })
    if (res.ok) {
      return await res.text()
    } else {
      throw new Error(`Failed to generate faucet invoice: ${await res.text()}`)
    }
  }
}
