import { WasmWorkerTransport } from '@fedimint/transport-web'
import { WalletDirector, type Transport } from '@fedimint/core'
import { TestFedimintWallet } from './TestFedimintWallet'

export class TestWalletDirector extends WalletDirector {
  constructor(client: Transport = new WasmWorkerTransport()) {
    super(client, undefined, true)
  }
  async createTestWallet(testId: string) {
    await this._client.initialize(testId)
    return new TestFedimintWallet(this._client)
  }
}
