import { TransportClient } from '../transport'
import type {
  Duration,
  JSONObject,
  JSONValue,
  MintSpendNotesResponse,
  MSats,
  NoteCountByDenomination,
  ReissueExternalNotesState,
  SpendNotesState,
} from '../types'

export class MintService {
  constructor(
    private client: TransportClient,
    private clientName: string,
  ) {}

  /** https://sdk.fedimint.org/core/FedimintWallet/MintService/redeemEcash */
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

  subscribeReissueExternalNotes(
    operationId: string,
    onSuccess: (state: ReissueExternalNotesState) => void = () => {},
    onError: (error: string) => void = () => {},
  ) {
    const unsubscribe = this.client.rpcStream<ReissueExternalNotesState>(
      'mint',
      'subscribe_reissue_external_notes',
      { operation_id: operationId },
      this.clientName,
      onSuccess,
      onError,
    )

    return unsubscribe
  }

  /** https://sdk.fedimint.org/core/FedimintWallet/MintService/spendNotes */
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

    const res = await this.client.rpcSingle<MintSpendNotesResponse>(
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
    const notes = res[1]
    const operationId = res[0]

    return {
      notes,
      operation_id: operationId,
    }
  }

  /** https://sdk.fedimint.org/core/FedimintWallet/MintService/parseEcash */
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
      this.clientName,
      (res) => onSuccess(res),
      onError,
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
