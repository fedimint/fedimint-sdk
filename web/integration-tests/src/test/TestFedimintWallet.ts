import { TransportClient } from '@fedimint/core'
import { FedimintWallet } from '@fedimint/core/testing'
import { TestingService } from './TestingService'

export class TestFedimintWallet extends FedimintWallet {
  public testing: TestingService

  constructor(_client: TransportClient) {
    super(_client)
    this.testing = new TestingService(_client, this.lightning)
  }

  async fundWallet(amountMSats: number) {
    const info = await this.testing.getFaucetGatewayInfo()
    const invoice = await this.lightning.createInvoice(
      amountMSats,
      '',
      1000,
      info,
    )
    await Promise.all([
      this.testing.payFaucetInvoice(invoice.invoice),
      this.lightning.waitForReceive(invoice.operation_id),
    ])
  }

  // Method to expose the TransportClient
  getTransportClient(): TransportClient {
    return this['_client']
  }
}
