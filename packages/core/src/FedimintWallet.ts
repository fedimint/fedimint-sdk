import { TransportClient } from './transport'
import {
  BalanceService,
  MintService,
  LightningService,
  FederationService,
  RecoveryService,
  WalletService,
} from './services'

// The Rpc requires exactly 36 length uuid strings
// This is temporary until we have a proper client management system
const DEFAULT_CLIENT_NAME = 'dd5135b2-c228-41b7-a4f9-3b6e7afe3088' as const

export type JoinFederationOptions = {
  clientName?: string
  forceRecover?: boolean
}

/**
 * Provides wallet operations and access to the wallet's services.
 *
 * Applications obtain instances from {@link WalletDirector.createWallet};
 * `FedimintWallet` is exported from `@fedimint/core` as a type, not as a
 * directly constructible runtime value.
 *
 * After creating an instance, call {@link FedimintWallet.open | open} to use
 * existing client state or {@link FedimintWallet.joinFederation | joinFederation}
 * to join a federation.
 */
export class FedimintWallet {
  public balance: BalanceService
  public mint: MintService
  public lightning: LightningService
  public federation: FederationService
  public recovery: RecoveryService
  public wallet: WalletService

  private _openPromise: Promise<void> | undefined = undefined
  private _resolveOpen: () => void = () => {}
  private _isOpen: boolean = false

  /**
   * Creates a wallet facade for a director-owned transport client.
   *
   * Application code should use {@link WalletDirector.createWallet} instead of
   * calling this constructor. The constructor is exported at
   * `@fedimint/core/testing` only for SDK tests.
   *
   * @param _client - The transport client owned and initialized by the director.
   * @param _clientName - The client name used by the wallet's services.
   */
  constructor(
    private _client: TransportClient,
    private _clientName: string = DEFAULT_CLIENT_NAME,
  ) {
    this._openPromise = new Promise((resolve) => {
      this._resolveOpen = resolve
    })
    this.mint = new MintService(this._client, this._clientName)
    this.lightning = new LightningService(this._client, this._clientName)
    this.balance = new BalanceService(this._client, this._clientName)
    this.federation = new FederationService(this._client, this._clientName)
    this.recovery = new RecoveryService(this._client, this._clientName)
    this.wallet = new WalletService(this._client, this._clientName)
  }

  async waitForOpen() {
    if (this._isOpen) return Promise.resolve()
    return this._openPromise
  }

  async open(clientName: string = DEFAULT_CLIENT_NAME) {
    // TODO: Determine if this should be safe or throw
    if (this._isOpen) throw new Error('The FedimintWallet is already open.')
    try {
      await this._client.sendSingleMessage('open_client', {
        client_name: clientName,
      })
      this._isOpen = true
      this._resolveOpen()
      return true
    } catch (e) {
      this._client.logger.error('Error opening client', e)
      throw e
    }
  }

  async joinFederation(
    inviteCode: string,
    clientName?: string,
  ): Promise<boolean>
  async joinFederation(
    inviteCode: string,
    options?: JoinFederationOptions,
  ): Promise<boolean>
  async joinFederation(
    inviteCode: string,
    clientNameOrOptions: string | JoinFederationOptions = DEFAULT_CLIENT_NAME,
  ): Promise<boolean> {
    const options =
      typeof clientNameOrOptions === 'string'
        ? {
            clientName: clientNameOrOptions,
            forceRecover: false,
          }
        : {
            clientName: clientNameOrOptions.clientName ?? DEFAULT_CLIENT_NAME,
            forceRecover: clientNameOrOptions.forceRecover ?? false,
          }

    // TODO: Determine if this should be safe or throw
    if (this._isOpen)
      throw new Error(
        'The FedimintWallet is already open. You can only call `joinFederation` on closed clients.',
      )
    try {
      await this._client.sendSingleMessage('join_federation', {
        invite_code: inviteCode,
        client_name: options.clientName,
        force_recover: options.forceRecover,
      })
      this._isOpen = true
      this._resolveOpen()

      return true
    } catch (e) {
      this._client.logger.error('Error joining federation', e)
      return false
    }
  }

  /**
   * This should ONLY be called when UNLOADING the wallet client.
   * After this call, the FedimintWallet instance should be discarded.
   */
  async cleanup() {
    this._openPromise = undefined
    this._isOpen = false
    await this._client.cleanup()
  }

  isOpen() {
    return this._isOpen
  }
}
