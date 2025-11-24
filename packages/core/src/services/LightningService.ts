import { TransportClient } from '../transport'
import type {
  CreateBolt11Response,
  GatewayInfo,
  JSONObject,
  LightningAddressSuccessAction,
  LightningAddressVerification,
  LightningGateway,
  LnInternalPayState,
  LnPayState,
  LnReceiveState,
  OutgoingLightningPayment,
} from '../types'

type PayLightningAddressOptions = {
  comment?: string
  extraMeta?: JSONObject
  gatewayInfo?: GatewayInfo
}

type PayLightningAddressResult = {
  invoice: string
  payment: OutgoingLightningPayment
  successAction?: LightningAddressSuccessAction
}

export class LightningService {
  constructor(
    private client: TransportClient,
    private clientName: string,
  ) {}

  /** https://sdk.fedimint.org/core/FedimintWallet/LightningService/createInvoice#lightning-createinvoice */
  async createInvoice(
    amountMsats: number,
    description: string,
    expiryTime?: number, // in seconds
    gatewayInfo?: GatewayInfo,
    extraMeta?: JSONObject,
  ) {
    const gateway = gatewayInfo ?? (await this._getDefaultGatewayInfo())
    return await this.client.rpcSingle<CreateBolt11Response>(
      'ln',
      'create_bolt11_invoice',
      {
        amount: amountMsats,
        description,
        expiry_time: expiryTime ?? null,
        extra_meta: extraMeta ?? {},
        gateway,
      },
      this.clientName,
    )
  }

  async createInvoiceTweaked(
    amountMsats: number,
    description: string,
    tweakKey: string,
    index: number,
    expiryTime?: number, // in seconds
    gatewayInfo?: GatewayInfo,
    extraMeta?: JSONObject,
  ) {
    const gateway = gatewayInfo ?? (await this._getDefaultGatewayInfo())
    return await this.client.rpcSingle<CreateBolt11Response>(
      'ln',
      'create_bolt11_invoice_for_user_tweaked',
      {
        amount: amountMsats,
        description,
        expiry_time: expiryTime ?? null,
        user_key: tweakKey,
        index,
        extra_meta: extraMeta ?? {},
        gateway,
      },
      this.clientName,
    )
  }

  // Returns the operation ids of payments received to the tweaks of the user secret key
  async scanReceivesForTweaks(
    tweakKey: string,
    indices: number[],
    extraMeta?: JSONObject,
  ) {
    return await this.client.rpcSingle<string[]>(
      'ln',
      'scan_receive_for_user_tweaked',
      {
        user_key: tweakKey,
        indices,
        extra_meta: extraMeta ?? {},
      },
      this.clientName,
    )
  }

  private async _getDefaultGatewayInfo() {
    await this.updateGatewayCache()
    const gateways = await this.listGateways()
    return gateways[0]?.info
  }

  /** https://sdk.fedimint.org/core/FedimintWallet/LightningService/payInvoice#lightning-payinvoice-invoice-string */
  async payInvoice(
    invoice: string,
    gatewayInfo?: GatewayInfo,
    extraMeta?: JSONObject,
  ) {
    const gateway = gatewayInfo ?? (await this._getDefaultGatewayInfo())
    return await this.client.rpcSingle<OutgoingLightningPayment>(
      'ln',
      'pay_bolt11_invoice',
      {
        maybe_gateway: gateway,
        invoice,
        extra_meta: extraMeta ?? {},
      },
      this.clientName,
    )
  }

  /** https://sdk.fedimint.org/core/FedimintWallet/LightningService/payInvoice#lightning-payinvoicesync-invoice-string */
  async payInvoiceSync(
    invoice: string,
    timeoutMs: number = 10000,
    gatewayInfo?: GatewayInfo,
    extraMeta?: JSONObject,
  ) {
    return new Promise<
      | { success: false; error?: string }
      | {
          success: true
          data: { feeMsats: number; preimage: string }
        }
    >(async (resolve, reject) => {
      const { contract_id, fee } = await this.payInvoice(
        invoice,
        gatewayInfo,
        extraMeta,
      )

      // TODO: handle  error handling for other subscription statuses
      const unsubscribe = this.subscribeLnPay(contract_id, (res) => {
        if (typeof res !== 'string' && 'success' in res) {
          clearTimeout(timeoutId)
          unsubscribe()
          resolve({
            success: true,
            data: { feeMsats: fee, preimage: res.success.preimage },
          })
        } else if (typeof res !== 'string' && 'unexpected_error' in res) {
          reject(new Error(res.unexpected_error.error_message))
        }
      })

      const timeoutId = setTimeout(() => {
        unsubscribe()
        resolve({ success: false, error: 'Payment timeout' })
      }, timeoutMs)
    })
  }

  subscribeInternalPayment(
    operation_id: string,
    onSuccess: (state: LnInternalPayState) => void = () => {},
    onError: (error: string) => void = () => {},
  ) {
    return this.client.rpcStream(
      'ln',
      'subscribe_internal_pay',
      { operation_id: operation_id },
      this.clientName,
      onSuccess,
      onError,
    )
  }

  // TODO: Document
  subscribeLnClaim(
    operationId: string,
    onSuccess: (state: LnReceiveState) => void = () => {},
    onError: (error: string) => void = () => {},
  ) {
    return this.client.rpcStream(
      'ln',
      'subscribe_ln_claim',
      { operation_id: operationId },
      this.clientName,
      onSuccess,
      onError,
    )
  }

  // TODO: Document (for external payments only)
  // TODO: Make this work for BOTH internal and external payments
  /** https://sdk.fedimint.org/core/FedimintWallet/LightningService/payInvoice#lightning-payinvoice-invoice-string */
  subscribeLnPay(
    operationId: string,
    onSuccess: (state: LnPayState) => void = () => {},
    onError: (error: string) => void = () => {},
  ) {
    return this.client.rpcStream(
      'ln',
      'subscribe_ln_pay',
      { operation_id: operationId },
      this.clientName,
      onSuccess,
      onError,
    )
  }

  /** https://sdk.fedimint.org/core/FedimintWallet/LightningService/payInvoice#lightning-payinvoice-invoice-string */
  async waitForPay(operationId: string) {
    return new Promise<
      | { success: false; error?: string }
      | { success: true; data: { preimage: string } }
    >((resolve, reject) => {
      let unsubscribe: () => void
      const timeoutId = setTimeout(() => {
        resolve({ success: false, error: 'Waiting for receive timeout' })
      }, 15000)

      unsubscribe = this.subscribeLnPay(
        operationId,
        (res) => {
          if (typeof res !== 'string' && 'success' in res) {
            clearTimeout(timeoutId)
            unsubscribe()
            resolve({
              success: true,
              data: { preimage: res.success.preimage },
            })
          }
        },
        (error) => {
          clearTimeout(timeoutId)
          unsubscribe()
          reject(error)
        },
      )
    })
  }

  /** https://sdk.fedimint.org/core/FedimintWallet/LightningService/createInvoice#lightning-createinvoice */
  subscribeLnReceive(
    operationId: string,
    onSuccess: (state: LnReceiveState) => void = () => {},
    onError: (error: string) => void = () => {},
  ) {
    return this.client.rpcStream(
      'ln',
      'subscribe_ln_receive',
      { operation_id: operationId },
      this.clientName,
      onSuccess,
      onError,
    )
  }

  /** https://sdk.fedimint.org/core/FedimintWallet/LightningService/createInvoice#lightning-createinvoice */
  async waitForReceive(operationId: string, timeoutMs: number = 15000) {
    return new Promise<LnReceiveState>((resolve, reject) => {
      let unsubscribe: () => void
      const timeoutId = setTimeout(() => {
        reject(new Error('Timeout waiting for receive'))
      }, timeoutMs)

      unsubscribe = this.subscribeLnReceive(
        operationId,
        (res) => {
          if (res === 'claimed') {
            clearTimeout(timeoutId)
            unsubscribe()
            resolve(res)
          }
        },
        (error) => {
          clearTimeout(timeoutId)
          unsubscribe()
          reject(error)
        },
      )
    })
  }

  async getGateway(
    gatewayId: string | null = null,
    forceInternal: boolean = false,
  ) {
    return await this.client.rpcSingle<LightningGateway | null>(
      'ln',
      'get_gateway',
      {
        gateway_id: gatewayId,
        force_internal: forceInternal,
      },
      this.clientName,
    )
  }

  async listGateways() {
    return await this.client.rpcSingle<LightningGateway[]>(
      'ln',
      'list_gateways',
      {},
      this.clientName,
    )
  }

  async updateGatewayCache() {
    return await this.client.rpcSingle(
      'ln',
      'update_gateway_cache',
      {},
      this.clientName,
    )
  }

  async verifyLightningAddress(
    lightningAddress: string,
  ): Promise<LightningAddressVerification> {
    return await this.client.rpcSingle<LightningAddressVerification>(
      'ln',
      'verify_lightning_address',
      {
        lightning_address: lightningAddress,
      },
      this.clientName,
    )
  }

  async payLightningAddress(
    lightningAddress: string,
    amountMsats: number,
    options: PayLightningAddressOptions = {},
  ): Promise<PayLightningAddressResult> {
    const gateway = options.gatewayInfo ?? (await this._getDefaultGatewayInfo())

    return await this.client.rpcSingle<PayLightningAddressResult>(
      'ln',
      'pay_lightning_address',
      {
        lightning_address: lightningAddress,
        amount_msats: amountMsats,
        comment: options.comment ?? null,
        extra_meta: options.extraMeta ?? {},
        gateway,
      },
      this.clientName,
    )
  }
}

export type { PayLightningAddressOptions, PayLightningAddressResult }
