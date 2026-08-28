import type {
  EcashTransaction,
  EventLogParams,
  JSONValue,
  LightningTransaction,
  LnVariant,
  MetaConsensusValue,
  MintVariant,
  OperationKey,
  OperationLog,
  PersistedLogEntry,
  Transactions,
  WalletTransaction,
  WalletVariant,
} from '../types'
import { TransportClient } from '../transport'

/**
 * Mirrors `fedimint_meta_common::DEFAULT_META_KEY`.
 */
export const DEFAULT_META_KEY = 0

export class FederationService {
  constructor(
    private client: TransportClient,
    private clientName: string,
  ) {}

  async getConfig() {
    return await this.client.rpcSingle('', 'get_config', {}, this.clientName)
  }

  async getFederationId() {
    return await this.client.rpcSingle<string>(
      '',
      'get_federation_id',
      {},
      this.clientName,
    )
  }

  async getInviteCode(peer: number = 0) {
    return await this.client.rpcSingle<string | null>(
      '',
      'get_invite_code',
      {
        peer,
      },
      this.clientName,
    )
  }

  async getMetaConsensusValue<T extends JSONValue = JSONValue>(
    key: number = DEFAULT_META_KEY,
  ): Promise<MetaConsensusValue<T> | null> {
    return await this.client.rpcSingle<MetaConsensusValue<T> | null>(
      'meta',
      'get_consensus_value',
      { key },
      this.clientName,
    )
  }

  async listOperations(
    limit?: number,
    last_seen?: OperationKey,
  ): Promise<[OperationKey, OperationLog][]> {
    return await this.client.rpcSingle<[OperationKey, OperationLog][]>(
      '',
      'list_operations',
      {
        limit: limit ?? null,
        last_seen: last_seen ?? null,
      },
      this.clientName,
    )
  }

  async getOperation(operationId: string) {
    return await this.client.rpcSingle<OperationLog | null>(
      '',
      'get_operation',
      { operation_id: operationId },
      this.clientName,
    )
  }

  async getEventLog(params: EventLogParams = {}): Promise<PersistedLogEntry[]> {
    return await this.client.rpcSingle<PersistedLogEntry[]>(
      '',
      'get_event_log',
      {
        pos: params.pos ?? null,
        limit: params.limit ?? null,
      },
      this.clientName,
    )
  }

  async listTransactions(
    limit?: number,
    last_seen?: OperationKey,
  ): Promise<Transactions[]> {
    const operations = await this.listOperations(limit, last_seen)
    return operations
      .filter(
        (item): item is [OperationKey, OperationLog] =>
          Array.isArray(item) && item.length === 2,
      )
      .filter(([_, op]) => {
        const { operation_module_kind, meta } = op
        const variant = meta.variant
        return (
          (operation_module_kind === 'ln' &&
            ((variant as LnVariant).pay || (variant as LnVariant).receive)) ||
          (operation_module_kind === 'mint' &&
            ((variant as MintVariant).spend_o_o_b ||
              (variant as MintVariant).reissuance)) ||
          (operation_module_kind === 'wallet' &&
            ((variant as WalletVariant).deposit ||
              (variant as WalletVariant).withdraw))
        )
      })
      .map(([key, op]) => {
        const timestamp = key.creation_time
          ? Math.round(
              key.creation_time.secs_since_epoch * 1000 +
                key.creation_time.nanos_since_epoch / 1_000_000,
            )
          : 0
        const operationId = key.operation_id
        const kind = op.operation_module_kind as 'ln' | 'mint' | 'wallet'
        const meta = op.meta
        const variant = meta.variant

        let outcome: string | undefined
        if (op.outcome && op.outcome.outcome) {
          if (typeof op.outcome.outcome === 'string') {
            outcome = op.outcome.outcome
          } else if (
            typeof op.outcome.outcome === 'object' &&
            op.outcome.outcome !== null
          ) {
            if ('success' in op.outcome.outcome) outcome = 'success'
            else if ('canceled' in op.outcome.outcome) outcome = 'canceled'
            else if ('claimed' in op.outcome.outcome) outcome = 'claimed'
            else if ('funded' in op.outcome.outcome) outcome = 'funded'
            else if ('awaiting_funds' in op.outcome.outcome)
              outcome = 'awaiting_funds'
            else if ('unexpected_error' in op.outcome.outcome)
              outcome = 'unexpected_error'
            else if ('created' in op.outcome.outcome) outcome = 'created'
            else if ('waiting_for_refund' in op.outcome.outcome)
              outcome = 'canceled'
            else if ('awaiting_change' in op.outcome.outcome)
              outcome = 'pending'
            else if ('refunded' in op.outcome.outcome) outcome = 'refunded'
            else if ('waiting_for_payment' in op.outcome.outcome)
              outcome = 'awaiting_funds'
            else if ('Created' in op.outcome.outcome) outcome = 'Created'
            else if ('Success' in op.outcome.outcome) outcome = 'Success'
            else if ('Refunded' in op.outcome.outcome) outcome = 'Refunded'
            else if ('UserCanceledProcessing' in op.outcome.outcome)
              outcome = 'UserCanceledProcessing'
            else if ('UserCanceledSuccess' in op.outcome.outcome)
              outcome = 'UserCanceledSuccess'
            else if ('UserCanceledFailure' in op.outcome.outcome)
              outcome = 'UserCanceledFailure'
            else if ('WaitingForTransaction' in op.outcome.outcome)
              outcome = 'pending'
            else if ('WaitingForConfirmation' in op.outcome.outcome)
              outcome = 'pending'
            else if ('Confirmed' in op.outcome.outcome) outcome = 'Confirmed'
            else if ('Claimed' in op.outcome.outcome) outcome = 'Claimed'
            else if ('Failed' in op.outcome.outcome) outcome = 'Failed'
          }
        }

        if (kind === 'ln') {
          const isPay = !!(variant as LnVariant).pay
          const txId =
            (variant as LnVariant).pay?.out_point.txid ||
            (variant as LnVariant).receive?.out_point.txid ||
            ''
          const type = (variant as LnVariant).pay ? 'send' : 'receive'
          const invoice =
            (variant as LnVariant).pay?.invoice ||
            (variant as LnVariant).receive?.invoice ||
            ''
          const gateway =
            (variant as LnVariant).pay?.gateway_id ||
            (variant as LnVariant).receive?.gateway_id ||
            ''
          const fee = (variant as LnVariant).pay?.fee
          const internalPay = (variant as LnVariant).pay?.is_internal_payment
          const preimage =
            isPay &&
            op.outcome?.outcome &&
            typeof op.outcome.outcome === 'object' &&
            'success' in op.outcome.outcome
              ? op.outcome.outcome.success.preimage
              : undefined

          return {
            timestamp,
            operationId,
            kind,
            txId,
            type,
            invoice,
            internalPay,
            fee,
            gateway,
            outcome: outcome as LightningTransaction['outcome'],
          } as LightningTransaction
        } else if (kind === 'mint') {
          const txId = (variant as MintVariant).reissuance?.txid
          const type = (variant as MintVariant).reissuance
            ? 'reissue'
            : 'spend_oob'
          const amountMsats = meta.amount
          const notes = (variant as MintVariant).spend_o_o_b?.oob_notes

          return {
            timestamp,
            type,
            txId,
            outcome: outcome as EcashTransaction['outcome'],
            operationId,
            amountMsats,
            notes,
            kind,
          } as EcashTransaction
        } else if (kind === 'wallet') {
          const type = (variant as WalletVariant).deposit
            ? 'deposit'
            : 'withdraw'
          const address =
            (variant as WalletVariant).deposit?.address ||
            (variant as WalletVariant).withdraw?.address ||
            ''
          const feeRate = (variant as WalletVariant).withdraw?.fee.fee_rate
            .sats_per_kvb
          const amountMsats =
            (variant as WalletVariant).withdraw?.amountMsats || 0

          return {
            timestamp,
            type,
            onchainAddress: address,
            fee: feeRate || 0,
            amountMsats,
            outcome,
            kind,
            operationId,
          } as WalletTransaction
        }
      })
      .filter(
        (transaction): transaction is Transactions => transaction !== undefined,
      )
  }
}
