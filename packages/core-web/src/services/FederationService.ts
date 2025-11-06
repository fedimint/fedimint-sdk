import type {
  EcashTransaction,
  LightningTransaction,
  LnVariant,
  MintVariant,
  OperationKey,
  OperationLog,
  Transactions,
  WalletTransaction,
  WalletVariant,
} from '../types'
import { RpcClient } from '../rpc'

export class FederationService {
  constructor(
    private client: RpcClient,
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

  async getSessionCount() {
    return await this.client.rpcSingle<number>(
      '',
      'session_count',
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
            ('pay' in variant || 'receive' in variant)) ||
          (operation_module_kind === 'mint' &&
            ('spend_o_o_b' in variant || 'reissuance' in variant)) ||
          (operation_module_kind === 'wallet' &&
            ('deposit' in variant || 'withdraw' in variant))
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
        const kind = op.operation_module_kind
        const meta = op.meta
        const variant = meta.variant
        let type, amount

        const outcome = determineOutcome(op.outcome)

        if (kind === 'ln') {
          type = 'pay' in variant ? 'pay' : 'receive'
          const payData = 'pay' in variant ? variant.pay : undefined
          const receiveData = 'receive' in variant ? variant.receive : undefined
          const isPay = !!('pay' in variant)
          const txId =
            payData?.out_point.txid || receiveData?.out_point.txid || ''
          const invoice = payData?.invoice || receiveData?.invoice || ''
          const gateway = payData?.gateway_id || receiveData?.gateway_id || ''
          const fee = payData?.fee
          const internalPay = payData?.is_internal_payment
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
          const reissuanceData =
            'reissuance' in variant ? variant.reissuance : undefined
          const spendData =
            'spend_o_o_b' in variant ? variant.spend_o_o_b : undefined
          const txId = reissuanceData?.txid
          type = reissuanceData ? 'reissue' : 'spend_oob'
          amount = meta.amount
          const notes = spendData?.oob_notes

          return {
            timestamp,
            type,
            txId,
            outcome: outcome as EcashTransaction['outcome'],
            operationId,
            amountMsats: amount,
            notes,
            kind,
          } as EcashTransaction
        } else if (kind === 'wallet') {
          const depositData = 'deposit' in variant ? variant.deposit : undefined
          const withdrawData =
            'withdraw' in variant ? variant.withdraw : undefined
          const type = depositData ? 'deposit' : 'withdraw'
          const address = depositData?.address || withdrawData?.address || ''
          const feeRate =
            ((withdrawData?.fee?.total_weight ?? 0) / 1000) *
            (withdrawData?.fee?.fee_rate?.sats_per_kvb ?? 0)
          const amount = withdrawData?.amount || 0

          return {
            timestamp,
            type,
            onchainAddress: address,
            fee: feeRate || 0,
            amountSats: amount,
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

function determineOutcome(outcome: unknown): string | undefined {
  if (!outcome || typeof outcome !== 'object' || !('outcome' in outcome)) {
    return undefined
  }

  const operationOutcome = (outcome as { outcome: unknown }).outcome

  if (typeof operationOutcome === 'string') {
    return operationOutcome
  }

  if (typeof operationOutcome !== 'object' || operationOutcome === null) {
    return undefined
  }

  const outcomeMap: Record<string, string> = {
    success: 'success',
    canceled: 'canceled',
    claimed: 'claimed',
    funded: 'funded',
    awaiting_funds: 'awaiting_funds',
    unexpected_error: 'unexpected_error',
    created: 'created',
    waiting_for_refund: 'canceled',
    awaiting_change: 'pending',
    refunded: 'refunded',
    waiting_for_payment: 'awaiting_funds',
    Created: 'Created',
    Success: 'Success',
    Refunded: 'Refunded',
    UserCanceledProcessing: 'UserCanceledProcessing',
    UserCanceledSuccess: 'UserCanceledSuccess',
    UserCanceledFailure: 'UserCanceledFailure',
    WaitingForTransaction: 'pending',
    WaitingForConfirmation: 'pending',
    Confirmed: 'Confirmed',
    Claimed: 'Claimed',
    Failed: 'Failed',
  }

  for (const [key, value] of Object.entries(outcomeMap)) {
    if (key in operationOutcome) {
      return value
    }
  }

  return undefined
}
