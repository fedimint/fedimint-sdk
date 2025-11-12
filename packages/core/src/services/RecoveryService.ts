import type { JSONValue } from '../types'
import { TransportClient } from '../transport'

export class RecoveryService {
  constructor(
    private client: TransportClient,
    private clientName: string,
  ) {}

  async hasPendingRecoveries() {
    return await this.client.rpcSingle<boolean>(
      '',
      'has_pending_recoveries',
      {},
      this.clientName,
    )
  }

  async waitForAllRecoveries() {
    await this.client.rpcSingle(
      '',
      'wait_for_all_recoveries',
      {},
      this.clientName,
    )
  }

  subscribeToRecoveryProgress(
    onSuccess: (progress: { module_id: number; progress: JSONValue }) => void,
    onError: (error: string) => void,
  ) {
    return this.client.rpcStream<{
      module_id: number
      progress: JSONValue
    }>(
      '',
      'subscribe_to_recovery_progress',
      {},
      this.clientName,
      onSuccess,
      onError,
    )
  }

  async backupToFederation(metadata?: JSONValue): Promise<void> {
    await this.client.rpcSingle(
      '',
      'backup_to_federation',
      { metadata: metadata ?? {} },
      this.clientName,
    )
  }
}
