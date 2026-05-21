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
   * Creates a new instance of FedimintWallet.
   *
   * This constructor initializes a FedimintWallet instance, which manages communication
   * with a Web Worker. The Web Worker is responsible for running WebAssembly code that
   * handles the core Fedimint Client operations.
   *
   * (default) When not in lazy mode, the constructor immediately initializes the
   * Web Worker and begins loading the WebAssembly module in the background. This
   * allows for faster subsequent operations but may increase initial load time.
   *
   * In lazy mode, the Web Worker and WebAssembly initialization are deferred until
   * the first operation that requires them, reducing initial overhead at the cost
   * of a slight delay on the first operation.
   *
   * @example
   * // Create a wallet with immediate initialization
   * const wallet = new FedimintWallet();
   * wallet.open();
   *
   * // Create a wallet with lazy initialization
   * const lazyWallet = new FedimintWallet(true);
   * // Some time later...
   * lazyWallet.initialize();
   * lazyWallet.open();
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
