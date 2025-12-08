import { TransportClient } from './transport'
import { type LogLevel } from './utils/logger'
import {
  Transport,
  ParsedInviteCode,
  ParsedBolt11Invoice,
  PreviewFederation,
  ParsedNoteDetails,
} from '@fedimint/types'
import { FedimintWallet } from './FedimintWallet'

export class WalletDirector {
  // Protected to allow for TestWalletDirector to access the client
  protected _client: TransportClient

  /**
   * Creates a new instance of WalletDirector.
   *
   * @param {Transport} [transport] - Optional worker client instance. Provide your
   *                         own to use a custom transport (e.g. React Native).
   *
   * @param {boolean} lazy - If true, delays Web Worker and WebAssembly initialization
   *                         until needed. Default is false.
   */
  constructor(transport: Transport, lazy: boolean = false) {
    if (!transport) {
      throw new Error('WalletDirector requires a transport implementation')
    }
    this._client = new TransportClient(transport)
    this._client.logger.info('WalletDirector instantiated')
    if (!lazy) {
      this.initialize()
    }
  }

  async initialize() {
    this._client.logger.info('Initializing TransportClient')
    await this._client.initialize()
    this._client.logger.info('TransportClient initialized')
  }

  // TODO: Make this stateful... handle listing/joining/opening/closing wallets at this level
  async createWallet() {
    await this._client.initialize()
    return new FedimintWallet(this._client)
  }

  /**
   * Previews a federation by fetching its configuration and identifier.
   *
   * It allows you to retrieve a federation's detailed configuration before deciding to join it.
   * This includes global settings, API endpoints, consensus version, metadata, and module configurations.
   *
   * @param {string} inviteCode - The federation invite code to preview.
   * @returns {Promise<PreviewFederation>}
   *          A promise that resolves to an object containing:
   *          - `config`: The federation configuration (JsonClientConfig) with:
   *            - `global`: Global configuration (API endpoints, consensus version, metadata)
   *            - `modules`: Module configurations (e.g., "ln", "mint", "wallet")
   *          - `federation_id`: The unique identifier of the federation
   *
   * @throws {Error} If the TransportClient encounters an issue during the preview process,
   *                 such as network errors or invalid invite code format.
   *
   * @example
   * const inviteCode = "fed11...";
   * const preview = await director.previewFederation(inviteCode);
   * console.log(preview.federation_id); // "15db8cb4f1ec..."
   * console.log(preview.config.global.meta.federation_name); // "My Federation"
   * console.log(preview.config.global.consensus_version);
   */
  async previewFederation(inviteCode: string): Promise<PreviewFederation> {
    await this._client.initialize()
    const response = await this._client.sendSingleMessage<PreviewFederation>(
      'preview_federation',
      { invite_code: inviteCode },
    )
    return response
  }

  /**
   * Sets the log level for the library.
   * @param level The desired log level ('DEBUG', 'INFO', 'WARN', 'ERROR', 'NONE').
   */
  setLogLevel(level: LogLevel) {
    this._client.logger.setLevel(level)
    this._client.logger.info(`Log level set to ${level}.`)
  }

  /**
   * Parses a federation invite code and retrieves its details.
   *
   * This method can be called immediately after WalletDirector initialization
   * without requiring an open wallet or joined federation. It simply parses
   * the invite code structure to extract information.
   *
   * @param {string} inviteCode - The invite code to be parsed.
   * @returns {Promise<ParsedInviteCode>}
   *          A promise that resolves to an object containing:
   *          - `federation_id`: The id of the federation
   *          - `url`: One of the API endpoints to connect to the federation
   *
   * @throws {Error} If the TransportClient encounters an issue during the parsing process.
   *
   * @example
   * const inviteCode = "fed11...";
   * const parsedCode = await director.parseInviteCode(inviteCode);
   * console.log(parsedCode.federation_id, parsedCode.url);
   */
  async parseInviteCode(inviteCode: string): Promise<ParsedInviteCode> {
    await this._client.initialize()
    const response = await this._client.sendSingleMessage<ParsedInviteCode>(
      'parse_invite_code',
      { invite_code: inviteCode },
    )
    return response
  }

  /**
   * Parses a BOLT11 Lightning invoice and retrieves its details.
   *
   * It simply parses the invoice structure to extract information.
   *
   * @param {string} invoiceStr - The BOLT11 invoice string to be parsed.
   * @returns {Promise<ParsedBolt11Invoice>}
   *          A promise that resolves to an object containing:
   *          - `amount`: The amount in satoshis (sats)
   *          - `expiry`: The expiry time in seconds
   *          - `memo`: A description or memo attached to the invoice
   *
   * @throws {Error} If the TransportClient encounters an issue during the parsing process.
   *
   * @example
   * const invoiceStr = "lnbc1...";
   * const parsedInvoice = await director.parseBolt11Invoice(invoiceStr);
   * console.log(parsedInvoice.amount, parsedInvoice.expiry, parsedInvoice.memo);
   */
  async parseBolt11Invoice(invoiceStr: string): Promise<ParsedBolt11Invoice> {
    await this._client.initialize()
    const response = await this._client.sendSingleMessage<ParsedBolt11Invoice>(
      'parse_bolt11_invoice',
      { invoice: invoiceStr },
    )
    return response
  }

  /**
   * Generates and sets a new mnemonic phrase.
   * @returns {Promise<string[]>} A promise that resolves to the generated mnemonic phrase.
   */
  async generateMnemonic(): Promise<string[]> {
    const result = await this._client.sendSingleMessage<{ mnemonic: string[] }>(
      'generate_mnemonic',
    )
    return result.mnemonic
  }

  /**
   * Retrieves the current mnemonic phrase.
   * @returns {Promise<string[]>} A promise that resolves to the current mnemonic phrase.
   */
  async getMnemonic(): Promise<string[]> {
    const result = await this._client.sendSingleMessage<{ mnemonic: string[] }>(
      'get_mnemonic',
    )
    return result.mnemonic
  }

  /**
   * Sets the mnemonic phrase.
   * @param {string[]} words - The mnemonic words to set.
   * @returns {Promise<boolean>} A promise that resolves to true if the mnemonic was set successfully.
   */
  async setMnemonic(words: string[]): Promise<boolean> {
    const result = await this._client.sendSingleMessage<{ success: boolean }>(
      'set_mnemonic',
      { words },
    )
    return result.success
  }

  /**
   * Parses OOB notes and retrieves their details.
   *
   * This method analyzes ecash notes to extract information about the total amount,
   * federation ID, invite code (if present), and note denomination breakdown.
   *
   * @param {string} notes - The OOB notes string to be parsed.
   * @returns {Promise<ParsedNoteDetails>}
   *          A promise that resolves to an object containing:
   *          - `total_amount`: The total amount of all notes in millisats
   *          - `federation_id_prefix`: 4-byte hex string identifying the federation
   *          - `federation_id`: Full 32-byte hex string (if invite is present)
   *          - `invite_code`: Bech32 encoded invite code starting with "fed1" (if present)
   *          - `note_counts`: Map of denomination amounts (as strings) to their counts
   *
   * @throws {Error} If the TransportClient encounters an issue during the parsing process.
   *
   * @example
   * const notes = "...OOB notes string...";
   * const parsedNotes = await director.parseOobNotes(notes);
   * console.log(parsedNotes.total_amount, parsedNotes.federation_id_prefix);
   * console.log(parsedNotes.note_counts); // e.g., { "1000": 5, "5000": 2 }
   */
  async parseOobNotes(notes: string): Promise<ParsedNoteDetails> {
    await this._client.initialize()
    const response = await this._client.sendSingleMessage<ParsedNoteDetails>(
      'parse_oob_notes',
      { oob_notes: notes },
    )
    return response
  }
}
