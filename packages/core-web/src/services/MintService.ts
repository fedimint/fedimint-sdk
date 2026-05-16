import { RpcClient } from '../rpc'
import type {
  Duration,
  JSONObject,
  JSONValue,
  MSats,
  NoteCountByDenomination,
  ReissueExternalNotesState,
  SpendNotesState,
} from '../types'

export class MintService {
  constructor(
    private client: RpcClient,
    private clientName: string,
  ) {}

  /** https://web.fedimint.org/core/FedimintWallet/MintService/redeemEcash */
  async redeemEcash(notes: string) {
    return await this.client.rpcSingle<string>(
      'mint',
      'reissue_external_notes',
      {
        oob_notes: notes, // "out of band notes"
        extra_meta: null,
      },
      this.clientName,
    )
  }

  async reissueExternalNotes(oobNotes: string, extraMeta: JSONObject = {}) {
    return await this.client.rpcSingle<string>(
      'mint',
      'reissue_external_notes',
      {
        oob_notes: oobNotes,
        extra_meta: extraMeta,
      },
      this.clientName,
    )
  }

  async spendExactDenominationNotes(
    denominations: number[],
    tryCancelAfter: number | Duration = 3600 * 24, // defaults to 1 day
    includeInvite: boolean = false,
    extraMeta: JSONValue = {},
  ) {
    const duration =
      typeof tryCancelAfter === 'number'
        ? { nanos: 0, secs: tryCancelAfter }
        : tryCancelAfter

    return await this.client.rpcSingle<string>(
      'mint',
      'spend_notes_with_exact_denominations',
      {
        denominations_msat: denominations,
        include_invite: includeInvite,
        try_cancel_after: duration,
        extra_meta: extraMeta,
      },
      this.clientName,
    )
  }

  subscribeReissueExternalNotes(
    operationId: string,
    onSuccess: (state: ReissueExternalNotesState) => void = () => {},
    onError: (error: string) => void = () => {},
  ) {
    const unsubscribe = this.client.rpcStream<ReissueExternalNotesState>(
      'mint',
      'subscribe_reissue_external_notes',
      { operation_id: operationId },
      onSuccess,
      onError,
      () => {},
      this.clientName,
    )

    return unsubscribe
  }

  /** https://web.fedimint.org/core/FedimintWallet/MintService/spendNotes */
  async spendNotes(
    amountMsats: number,
    // Tells the wallet to automatically try to cancel the spend if it hasn't completed
    // after the specified number of seconds. If the receiver has already redeemed
    // the notes at this time, the notes will not be cancelled.
    tryCancelAfter: number | Duration = 3600 * 24, // defaults to 1 day
    includeInvite: boolean = false,
    extraMeta: JSONValue = {},
  ) {
    const duration =
      typeof tryCancelAfter === 'number'
        ? { nanos: 0, secs: tryCancelAfter }
        : tryCancelAfter

    const [operationId, notes] = await this.client.rpcSingle<[string, string]>(
      'mint',
      'spend_notes',
      {
        amount: amountMsats,
        try_cancel_after: duration,
        include_invite: includeInvite,
        extra_meta: extraMeta,
      },
      this.clientName,
    )

    return {
      notes,
      operation_id: operationId,
    }
  }

  /** https://web.fedimint.org/core/FedimintWallet/MintService/parseEcash */
  async parseNotes(oobNotes: string) {
    return await this.client.rpcSingle<MSats>(
      'mint',
      'validate_notes',
      {
        oob_notes: oobNotes,
      },
      this.clientName,
    )
  }

  async tryCancelSpendNotes(operationId: string) {
    await this.client.rpcSingle(
      'mint',
      'try_cancel_spend_notes',
      {
        operation_id: operationId,
      },
      this.clientName,
    )
  }

  subscribeSpendNotes(
    operationId: string,
    onSuccess: (state: SpendNotesState) => void = () => {},
    onError: (error: string) => void = () => {},
  ) {
    return this.client.rpcStream<SpendNotesState>(
      'mint',
      'subscribe_spend_notes',
      { operation_id: operationId },
      (res) => onSuccess(res),
      onError,
      () => {},
      this.clientName,
    )
  }

  async awaitSpendOobRefund(operationId: string) {
    return await this.client.rpcSingle(
      'mint',
      'await_spend_oob_refund',
      {
        operation_id: operationId,
      },
      this.clientName,
    )
  }

  async getNotesByDenomination() {
    return await this.client.rpcSingle<NoteCountByDenomination>(
      'mint',
      'note_counts_by_denomination',
      {},
      this.clientName,
    )
  }
}
