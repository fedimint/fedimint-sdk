import { MSats, Duration, JSONValue, JSONObject } from './utils'

const MODULE_KINDS = ['', 'ln', 'mint', 'wallet'] as const
type ModuleKind = (typeof MODULE_KINDS)[number]

type GatewayInfo = {
  gateway_id: string
  api: string
  node_pub_key: string
  federation_index: number
  route_hints: RouteHint[]
  fees: FeeToAmount
}

type LightningGateway = {
  info: GatewayInfo
  vetted: boolean
  ttl: Duration
}

type RouteHint = {
  // TODO: Define the structure of RouteHint
}

type FeeToAmount = {
  // TODO: Define the structure of FeeToAmount
}

type OutgoingLightningPayment = {
  payment_type: PayType
  contract_id: string
  fee: MSats
}

type PayType = { lightning: string } | { internal: string }

type GetAvailableGatewayParams = {
  gateway?: LightningGateway
  invoice?: string
}

type LnPayState =
  | 'created'
  | 'canceled'
  | { funded: { block_height: number } }
  | { waiting_for_refund: { error_reason: string } }
  | 'awaiting_change'
  | { success: { preimage: string } }
  | { refunded: { gateway_error: string } }
  | { unexpected_error: { error_message: string } }

type LnReceiveState =
  | 'created'
  | { waiting_for_payment: { invoice: string; timeout: number } }
  | { canceled: { reason: string } }
  | 'funded'
  | 'awaiting_funds'
  | 'claimed'

type LnInternalPayState =
  | 'funding'
  | { preimage: string }
  | { refund_success: { out_points: BtcOutPoint[]; error: string } }
  | { refund_error: { error_message: string; error: string } }
  | { funding_failed: { error: string } }
  | { unexpected_error: string }

type CreateBolt11Response = {
  operation_id: string
  invoice: string
}

type StreamError = {
  error: string
  data: never
  end: never
}

type StreamSuccess<T extends JSONValue> = {
  data: T
  error: never
  end: never
}

type StreamEnd = {
  end: string
  data: never
  error: never
}

type StreamResult<T extends JSONValue> =
  | StreamSuccess<T>
  | StreamError
  | StreamEnd

type CancelFunction = () => void

type ReissueExternalNotesState = 'Created' | 'Issuing' | 'Done'
// | { Failed: { error: string } }

type MintSpendNotesResponse = Array<string>

type SpendNotesState =
  | 'Created'
  | 'UserCanceledProcessing'
  | 'UserCanceledSuccess'
  | 'UserCanceledFailure'
  | 'Success'
  | 'Refunded'

type TxOutputSummary = {
  outpoint: {
    txid: string
    vout: number
  }
  amount: number
}

type BtcOutPoint = {
  txid: string
  vout: number
}

type WalletDepositState =
  | 'WaitingForTransaction'
  | {
      WaitingForConfirmation: {
        btc_deposited: number
        btc_out_point: BtcOutPoint
      }
    }
  | { Confirmed: { btc_deposited: number; btc_out_point: BtcOutPoint } }
  | { Claimed: { btc_deposited: number; btc_out_point: BtcOutPoint } }
  | { Failed: string }

type WalletSummary = {
  spendable_utxos: TxOutputSummary[]
  unsigned_peg_out_txos: TxOutputSummary[]
  unsigned_change_utxos: TxOutputSummary[]
  // TODO: fix typo in rust
  unconfirmed_peg_out_txos: TxOutputSummary[]
  unconfirmed_change_utxos: TxOutputSummary[]
}

type LnVariant = {
  pay?: {
    gateway_id: string
    invoice: string
    fee: number
    is_internal_payment: boolean
    out_point: {
      out_idx: number
      txid: string
    }
  }
  receive?: {
    gateway_id: string
    invoice: string
    out_point: {
      out_idx: number
      txid: string
    }
  }
}

type MintVariant = {
  spend_o_o_b?: {
    requested_amount: number
    oob_notes: string
  }
  reissuance?: {
    txid: string
  }
}

type WalletVariant = {
  deposit?: {
    address: string
    tweak_idx: number
  }
  withdraw?: {
    address: string
    amountMsats: number
    fee: {
      fee_rate: {
        sats_per_kvb: number
      }
      total_weight: number
    }
  }
}

type OperationKey = {
  creation_time: { nanos_since_epoch: number; secs_since_epoch: number }
  operation_id: string
}
type OperationMeta = {
  amount: number
  extra_meta: JSONObject
  variant: LnVariant | MintVariant | WalletVariant
}

type OperationLog = {
  meta: OperationMeta
  operation_module_kind: string
  outcome: {
    outcome: LnPayState | LnReceiveState | SpendNotesState | WalletDepositState
  }
}

type BaseTransactions = {
  timestamp: number
  operationId: string
  kind: 'ln' | 'mint' | 'wallet'
}

type LightningTransaction = BaseTransactions & {
  type: 'send' | 'receive'
  invoice: string
  outcome:
    | 'created'
    | 'canceled'
    | 'claimed'
    | 'pending'
    | 'success'
    | 'funded'
    | 'awaiting_funds'
    | 'unexpected_error'
  gateway: string
  fee?: number
  internalPay?: boolean
  preimage?: string
  txId: string
}

type EcashTransaction = BaseTransactions & {
  type: 'spend_oob' | 'reissue'
  amountMsats: number
  outcome?: SpendNotesState | ReissueExternalNotesState
  notes?: string
  txId?: string
}

type WalletTransaction = BaseTransactions & {
  type: 'withdraw' | 'deposit'
  onchainAddress: string
  amountMsats: number
  fee: number
  outcome?:
    | 'WaitingForTransaction'
    | 'WaitingForConfirmation'
    | 'Confirmed'
    | 'Claimed'
    | 'Failed'
}

type Transactions = LightningTransaction | EcashTransaction | WalletTransaction

/** Keys are powers of 2 */
type NoteCountByDenomination = Record<number, number>

type GenerateAddressResponse = {
  deposit_address: string
  operation_id: string
}

export {
  LightningGateway,
  RouteHint,
  FeeToAmount,
  OutgoingLightningPayment,
  PayType,
  LnPayState,
  LnReceiveState,
  CreateBolt11Response,
  GatewayInfo,
  StreamError,
  StreamSuccess,
  StreamResult,
  ModuleKind,
  CancelFunction,
  LnInternalPayState,
  ReissueExternalNotesState,
  MintSpendNotesResponse,
  SpendNotesState,
  WalletSummary,
  TxOutputSummary,
  NoteCountByDenomination,
  GenerateAddressResponse,
  OperationKey,
  OperationLog,
  LnVariant,
  MintVariant,
  WalletVariant,
  LightningTransaction,
  EcashTransaction,
  WalletTransaction,
  Transactions,
  WalletDepositState,
  GetAvailableGatewayParams,
}
