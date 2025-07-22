import { WalletSummary } from '../types'
import { RpcClient } from '../rpc'

export class WalletService {
  constructor(
    private client: RpcClient,
    private clientName?: string,
  ) {}

  async getWalletSummary(): Promise<WalletSummary> {
    return await this.client.rpcSingle(
      'wallet',
      'get_wallet_summary',
      {},
      this.clientName,
    )
  }
}
