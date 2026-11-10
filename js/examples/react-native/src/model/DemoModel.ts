import { useEffect, useRef, useSyncExternalStore } from 'react'
import {
  ActivityItem,
  Capabilities,
  Cursor,
  EcashQuoteLike,
  EcashSendOperationLike,
  EcashSendState,
  ErrorCode,
  FederationInfo,
  FederationLike,
  FederationPreview,
  FederationStatus,
  FederationStatus_Tags,
  InviteCode,
  InviteCodeLike,
  LnQuoteLike,
  LnReceiveState,
  LnSendState,
  Network,
  Notes,
  NotesLike,
  OnchainDepositsLike,
  OnchainQuoteLike,
  OnchainReceiveState,
  OnchainSendState,
  RecoveryState,
  RecoveryState_Tags,
  SdkLike,
  EcashReceiveState_Tags,
  openSdk,
  type SdkSession,
} from '@fedimint/react-native'
import { describe, isAbort, reason, sdkError } from './errors'
import {
  hasStoredWallet,
  moveLegacyWallet,
  prepareDataDir,
  readLastFederationId,
  recoversOnJoin,
  writeLastFederationId,
  type WalletOrigin,
} from './storage'
import { authenticateOwner } from './auth'

/** The mutinynet federation the iOS and Android demos also pre-fill. */
const testnetFederationCode =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqq' +
  'ysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

/** Names a result line, so each section can render its own outcome. */
export type Section =
  | 'wallet'
  | 'join'
  | 'lnReceive'
  | 'ecashReceive'
  | 'ecashSend'
  | 'lnPay'
  | 'deposit'
  | 'onchainSend'
  | 'activity'
  /** Reclaiming the last ecash send, kept apart from `ecashSend`. */
  | 'ecashReclaim'
  /** Following, or resuming, the attached federation's seed recovery. */
  | 'recovery'
  /** Attaching another federation this storage already holds. */
  | 'switching'

/**
 * Sections whose failure is a passing note under a form or a list, so it clears itself after
 * `failureLifetime`. The rest stay, because the failure is all their screen has to say: a send's
 * is its outcome, without which the result screen goes back to "on its way"; a wallet that
 * failed to open is why onboarding is showing; and a recovery that could not be read is why Home
 * is locked.
 */
const expiringFailures = new Set<Section>([
  'join',
  'switching',
  'lnReceive',
  'ecashReceive',
  'deposit',
  'ecashReclaim',
])

/**
 * Sections whose contents belong to one federation, and are therefore meaningless the moment
 * another one is attached. `wallet` is SDK-scoped, so it survives a switch; `join` and
 * `switching` are written by the caller of `attach` immediately afterwards.
 */
const federationScoped: Section[] = [
  'lnReceive',
  'ecashReceive',
  'ecashSend',
  'lnPay',
  'deposit',
  'onchainSend',
  'activity',
  'ecashReclaim',
  'recovery',
]

/** How long a passing failure stays on screen; see `expiringFailures`. */
const failureLifetime = 30_000
/** How many rows one page of history asks for. */
const activityPageSize = 20
/** What a section's line reads while its action runs. */
const workingText = 'working…'

/** Everything the screens render. One immutable object, replaced on every change. */
interface DemoState {
  results: Partial<Record<Section, string>>
  /** Why each section's last action failed, if it did. */
  failures: Partial<Record<Section, string>>
  /**
   * Why a section stopped following its operation. Set when the subscription failed, not the
   * call that started the operation: the operation may still have completed.
   */
  watchFailures: Partial<Record<Section, string>>
  /** Sections whose action is running. */
  working: ReadonlySet<Section>

  seedWords: string[]
  /** Why the phrase stayed hidden: no passcode, or authentication declined. */
  seedFailure?: string

  balanceMsats?: bigint
  balanceFailure?: string
  federationName?: string
  federationId?: string
  /** The attached federation's invite code. A credential, so shown only on request. */
  federationInvite?: string
  federationNetwork?: Network
  /** Which rails the federation has. Screens show only these. */
  capabilities?: Capabilities
  /** The attached federation's status, kept live by the status subscription. */
  federationStatus?: FederationStatus
  /** Every federation this storage holds, running or not, in no particular order. */
  storedFederations: FederationInfo[]
  /** The federation the last switch was for; under way while `switching` is working. */
  switchTarget?: string
  /** The attached federation's seed recovery, while one is being followed. */
  recoveryState?: RecoveryState
  activityItems: ActivityItem[]
  /** Where the next page of history starts, or `undefined` at its end. */
  activityNext?: Cursor

  // Inputs.
  restoreWords: string
  invite: string
  /** The last preview of `invite`. */
  invitePreview?: FederationPreview
  lnReceiveAmount: string
  lnReceiveDescription: string
  ecashNotes: string
  ecashSendAmount: string
  lnInvoice: string
  onchainAmount: string
  onchainAddress: string

  // Copyable results of the last run of each flow.
  lastInvoice?: string
  lastNotes?: NotesLike
  lastAddress?: string
  lnReceiveState?: LnReceiveState
  lnReceiveExpiresAt?: bigint
  /** The invoice's amount, which can differ from the one asked for. */
  lnReceiveInvoiceMsats?: bigint
  /** Whether the wallet has found a payment to `lastAddress` yet. */
  depositFound: boolean
  depositState?: OnchainReceiveState

  lnQuote?: LnQuoteLike
  onchainQuote?: OnchainQuoteLike
  ecashQuote?: EcashQuoteLike
  lnSendState?: LnSendState
  onchainSendState?: OnchainSendState
  ecashSendState?: EcashSendState

  /** Counts attaches, so a screen about the attached federation can tell it was replaced. */
  attachments: number
  isSendingEcash: boolean
  isPayingLightning: boolean
  isSendingOnchain: boolean
  /** Wallet opens, joins, recoveries and switches in flight. A count, as on iOS. */
  attachesInFlight: number

  hasSdk: boolean
  hasFederation: boolean
  walletOrigin?: WalletOrigin
  /** Whether Join recovers rather than joins plainly; see `recoversOnJoin` in storage. */
  recoversOnJoin: boolean
  /**
   * Set once Back up and Verify are behind a newly created wallet. Not persisted: a wallet
   * reopened later is `reopened`, not `created`, so the backup step is offered once. Kept here
   * rather than in the root view, because on Android the JS runtime, and so the open wallet,
   * outlives the activity: a relaunch after the back button remounts the root alone.
   */
  backedUp: boolean
}

const initialState: DemoState = {
  results: {},
  failures: {},
  watchFailures: {},
  working: new Set(),
  seedWords: [],
  storedFederations: [],
  activityItems: [],
  restoreWords: '',
  invite: testnetFederationCode,
  lnReceiveAmount: '5000',
  lnReceiveDescription: 'Coffee',
  ecashNotes: '',
  ecashSendAmount: '1000',
  lnInvoice: '',
  onchainAmount: '10000',
  onchainAddress: '',
  depositFound: false,
  attachments: 0,
  isSendingEcash: false,
  isPayingLightning: false,
  isSendingOnchain: false,
  attachesInFlight: 0,
  hasSdk: false,
  hasFederation: false,
  recoversOnJoin: true,
  backedUp: false,
}

/** The pull cursor every generated `…Updates` object has. */
interface Updates<T> {
  next(asyncOpts_?: { signal: AbortSignal }): Promise<T | undefined>
}

/** Joining the federation that is already attached and running. */
class AlreadyAttached extends Error {
  constructor() {
    super("You're already in this federation.")
  }
}

/**
 * Every SDK handle the screens hold, and one `run` helper that funnels each action's outcome,
 * including an SDK error, into one result line. The TypeScript twin of the iOS demo's
 * `DemoModel`: the same sections, guards and rules, so both demos behave the same.
 *
 * Unlike the Swift bindings, every SDK call here takes an `AbortSignal`, so a superseded run or
 * a replaced watcher is actually cancelled, not only ignored when it returns.
 */
export class DemoModel {
  private state = initialState
  private listeners = new Set<() => void>()

  private session?: SdkSession
  private federation?: FederationLike
  private lastEcashSend?: EcashSendOperationLike

  private runs = new Map<Section, AbortController>()
  /** Which run of a section is the current one. */
  private runGenerations = new Map<Section, number>()
  /** One long-lived operation watcher per section. */
  private watchers = new Map<Section, AbortController>()
  private balanceWatch?: AbortController
  private statusWatch?: AbortController
  private activityRefresh?: AbortController
  private revealGeneration = 0
  private lastFederationId?: string

  // MARK: Store

  getState = (): DemoState => this.state
  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }
  private set(patch: Partial<DemoState>) {
    this.state = { ...this.state, ...patch }
    for (const listener of this.listeners) listener()
  }
  private setIn<K extends 'results' | 'failures' | 'watchFailures'>(
    key: K,
    section: Section,
    value: string | undefined,
  ) {
    const next = { ...this.state[key] }
    if (value === undefined) delete next[section]
    else next[section] = value
    this.set({ [key]: next } as Partial<DemoState>)
  }

  isWorking(section: Section): boolean {
    return this.state.working.has(section)
  }
  /** Whether a send on any rail is starting. Joining or reopening waits for it. */
  get isSending(): boolean {
    const s = this.state
    return s.isSendingEcash || s.isPayingLightning || s.isSendingOnchain
  }
  get isAttaching(): boolean {
    return this.state.attachesInFlight > 0
  }
  private attachStarted() {
    this.set({ attachesInFlight: this.state.attachesInFlight + 1 })
  }
  private attachEnded() {
    this.set({ attachesInFlight: this.state.attachesInFlight - 1 })
  }

  // MARK: Inputs

  /** Editing the code drops the preview taken of the previous one. */
  setInvite(invite: string) {
    if (invite === this.state.invite) return
    this.set({ invite, invitePreview: undefined })
  }
  setRestoreWords(restoreWords: string) {
    this.set({ restoreWords })
  }
  setLnReceive(patch: { amount?: string; description?: string }) {
    this.set({
      lnReceiveAmount: patch.amount ?? this.state.lnReceiveAmount,
      lnReceiveDescription:
        patch.description ?? this.state.lnReceiveDescription,
    })
  }
  setEcashNotes(ecashNotes: string) {
    this.set({ ecashNotes })
  }
  /** A quote is for the amount it was taken for, so editing it drops the quote. */
  setEcashSendAmount(ecashSendAmount: string) {
    if (ecashSendAmount === this.state.ecashSendAmount) return
    this.set({ ecashSendAmount, ecashQuote: undefined })
  }
  /** Editing the invoice invalidates a quote taken against the previous one. */
  setLnInvoice(lnInvoice: string) {
    if (lnInvoice === this.state.lnInvoice) return
    this.set({ lnInvoice, lnQuote: undefined })
  }
  setOnchain(patch: { amount?: string; address?: string }) {
    const amount = patch.amount ?? this.state.onchainAmount
    const address = patch.address ?? this.state.onchainAddress
    if (
      amount === this.state.onchainAmount &&
      address === this.state.onchainAddress
    )
      return
    this.set({
      onchainAmount: amount,
      onchainAddress: address,
      onchainQuote: undefined,
    })
  }

  // MARK: Wallet

  /**
   * Whether a wallet from an earlier launch is waiting to be reopened. Rejects when a wallet the
   * old example left in Documents could not be moved: going on would offer a new wallet, whose
   * directory then stops the move for good and leaves the old seed in iCloud backups.
   */
  async start(): Promise<boolean> {
    await moveLegacyWallet()
    return hasStoredWallet()
  }

  openWallet() {
    if (this.isSending || this.isAttaching) return
    this.attachStarted()
    this.run('wallet', async (signal) => {
      try {
        const words = this.state.restoreWords.split(/\s+/).filter(Boolean)
        // Read before the open, which is what fills an empty directory.
        const origin: WalletOrigin =
          words.length > 0
            ? 'restored'
            : (await hasStoredWallet())
              ? 'reopened'
              : 'created'
        const dataDir = await prepareDataDir()
        const session = await openSdk({
          dataDir,
          mnemonic: words.length ? words : undefined,
        })
        if (signal.aborted) return 'superseded by a newer open'
        this.session = session
        this.lastFederationId = await readLastFederationId()
        this.set({
          hasSdk: true,
          walletOrigin: origin,
          recoversOnJoin: await recoversOnJoin(origin),
          // The words are the seed. Once the SDK holds it there is no reason for a copy to
          // stay in an input field.
          restoreWords: '',
        })
        this.followFederationStatuses(session.sdk)
        // Reattach to the federation used last. While that one is stored but not open, such
        // as quarantined, another stands in without being remembered in its place, and
        // Federations offers to reconnect it. Otherwise the stand-in takes over.
        const open = session.sdk.federations()
        const last = open.find((f) => f.id() === this.lastFederationId)
        const existing = last ?? open[0]
        if (existing) {
          const lastStored = this.state.storedFederations.some(
            (f) => f.id === this.lastFederationId,
          )
          this.attach(existing, last !== undefined || !lastStored)
          // A recovery interrupted by the last quit carries on in the SDK; follow it.
          if (
            this.state.federationStatus?.tag ===
            FederationStatus_Tags.Recovering
          ) {
            this.followRecovery()
          }
        }
        return words.length
          ? `wallet restored from ${words.length} words`
          : 'wallet opened'
      } finally {
        this.attachEnded()
      }
    })
  }

  /**
   * Shows the recovery phrase after device-owner authentication: Face ID or Touch ID, falling
   * back to the passcode. Hiding it needs nothing. It fails closed: a device with no passcode
   * cannot authenticate the owner, so it never shows the phrase.
   */
  toggleSeed() {
    if (this.state.seedWords.length > 0) {
      this.hideSeed()
      return
    }
    if (!this.session) return
    // A new attempt, so an earlier one's prompt or failure timer stands down.
    const generation = ++this.revealGeneration
    this.set({ seedFailure: undefined })
    authenticateOwner("Show the wallet's recovery phrase").then((outcome) => {
      if (outcome !== 'ok') {
        this.failSeed(
          outcome === 'unavailable'
            ? 'set a device passcode to view the seed'
            : `seed stays hidden: ${outcome.declined}`,
          generation,
        )
        return
      }
      // Hidden while the prompt was up: the phrase must not appear behind that.
      if (generation !== this.revealGeneration || !this.session) return
      this.set({ seedWords: this.session.sdk.exportMnemonic().words() })
    })
  }

  private failSeed(reasonText: string, generation: number) {
    if (generation !== this.revealGeneration) return
    this.set({ seedFailure: reasonText })
    this.expire(
      () =>
        this.revealGeneration === generation &&
        this.state.seedFailure === reasonText,
      () => this.set({ seedFailure: undefined }),
    )
  }

  /** Back up and Verify are done for the wallet just created. */
  markBackedUp() {
    this.set({ backedUp: true })
  }

  /** Hides the phrase, and drops a reveal still waiting on authentication. */
  hideSeed() {
    this.revealGeneration += 1
    this.set({ seedWords: [], seedFailure: undefined })
  }

  // MARK: Federations

  preview() {
    if (this.state.isSendingEcash || this.isWorking('join')) return
    this.run('join', async (signal) => {
      const sdk = this.session?.sdk
      if (!sdk) return 'open the wallet first'
      const invite = this.state.invite.trim()
      const preview = await sdk.preview(InviteCode.parse(invite), { signal })
      if (signal.aborted || invite !== this.state.invite.trim()) {
        return 'superseded by a newer preview'
      }
      this.set({ invitePreview: preview })
      return `Previewed ${preview.id}`
    })
  }

  /**
   * The pasted invite, unless it is for the federation attached and running: reattaching that
   * would change nothing, and Join would close without a word.
   */
  private inviteToAttach(): InviteCodeLike {
    const code = InviteCode.parse(this.state.invite.trim())
    if (
      code.federationId() === this.state.federationId &&
      this.state.federationStatus?.tag === FederationStatus_Tags.Running
    ) {
      throw new AlreadyAttached()
    }
    return code
  }

  /** Joins the pasted federation, or reattaches if this storage already joined it. */
  join(): boolean {
    if (this.isSending || this.isAttaching || this.isWorking('join'))
      return false
    this.attachStarted()
    this.run('join', async (signal) => {
      try {
        const sdk = this.session?.sdk
        if (!sdk) return 'open the wallet first'
        const code = this.inviteToAttach()
        let joined: FederationLike
        let verb = 'Joined'
        try {
          joined = await sdk.join(code, { signal })
        } catch (error) {
          if (sdkError(error)?.code !== ErrorCode.AlreadyJoined) throw error
          // `reopenFederation` returns the live handle of an open federation, and brings back
          // a quarantined or closed one.
          joined = await sdk.reopenFederation(code.federationId(), { signal })
          verb = 'Already joined, reattached to'
        }
        if (signal.aborted) return 'superseded by a newer join'
        this.attach(joined)
        if (
          this.state.federationStatus?.tag === FederationStatus_Tags.Recovering
        ) {
          this.followRecovery()
        }
        return `${verb} ${joined.id()}`
      } finally {
        this.attachEnded()
      }
    })
    return true
  }

  /**
   * Joins with a seed-recovery rescan instead of a plain join: the path for a restored seed that
   * already holds funds here. Spends and receives are refused until the rescan completes.
   */
  recover(): boolean {
    if (this.isSending || this.isAttaching || this.isWorking('join'))
      return false
    this.attachStarted()
    this.run('join', async (signal) => {
      try {
        const sdk = this.session?.sdk
        if (!sdk) return 'open the wallet first'
        const code = this.inviteToAttach()
        let recovery
        try {
          recovery = await sdk.recover(code, { signal })
        } catch (error) {
          if (sdkError(error)?.code !== ErrorCode.AlreadyJoined) throw error
          // Joined already: switch back to it, as `join` does, and follow its recovery.
          const existing = await sdk.reopenFederation(code.federationId(), {
            signal,
          })
          if (signal.aborted) return 'superseded by a newer join'
          this.attach(existing)
          this.followRecovery()
          return `Already joined, reattached to ${existing.id()}`
        }
        if (signal.aborted) return 'superseded by a newer join'
        this.attach(recovery.federation)
        // The watcher's first update is the current state, so no separate read.
        this.watch('recovery', recovery.progress.updates(), (state) =>
          this.recoveryProgressed(state),
        )
        return `Recovering ${recovery.federation.id()}`
      } finally {
        this.attachEnded()
      }
    })
    return true
  }

  /**
   * Attaches another federation this storage holds. `reopenFederation` returns the live handle
   * of an open federation, and brings back a closed or quarantined one without its invite code.
   */
  switchFederation(id: string): boolean {
    const sdk = this.session?.sdk
    if (this.isSending || this.isAttaching || !sdk) return false
    this.set({ switchTarget: id })
    this.attachStarted()
    this.run('switching', async (signal) => {
      try {
        const reopened = await sdk.reopenFederation(id, { signal })
        if (signal.aborted) return 'superseded by a newer switch'
        this.attach(reopened)
        if (
          this.state.federationStatus?.tag === FederationStatus_Tags.Recovering
        ) {
          this.followRecovery()
        }
        return `switched to ${reopened.id()}`
      } finally {
        this.attachEnded()
      }
    })
    return true
  }

  // MARK: Lightning in

  lnReceive() {
    if (this.isWorking('lnReceive')) return
    this.run('lnReceive', async (signal) => {
      const lightning = this.federation?.lightning()
      if (!lightning) return 'this federation has no lightning module'
      const msats = parseAmount(this.state.lnReceiveAmount)
      if (msats === undefined) return 'enter an amount in msats'
      const receive = await lightning.receive(
        msats,
        this.state.lnReceiveDescription,
        { signal },
      )
      if (signal.aborted) return 'superseded by a newer request'
      // Before the state below is reset, and not before the invoice exists, so a request that
      // fails leaves the previous one followed.
      this.stopWatching('lnReceive')
      this.set({
        lastInvoice: receive.invoice,
        lnReceiveState: undefined,
        lnReceiveInvoiceMsats: undefined,
        lnReceiveExpiresAt: undefined,
      })
      // The invoice is already usable without these, which only describe it.
      const details = await receive.operation
        .details({ signal })
        .catch(() => undefined)
      if (signal.aborted) return 'superseded by a newer request'
      this.set({
        lnReceiveExpiresAt: details?.expiresAt,
        lnReceiveInvoiceMsats: details?.invoiceAmount,
      })
      this.watch('lnReceive', receive.operation.updates(), (state) =>
        this.set({ lnReceiveState: state }),
      )
      return `Invoice:\n${receive.invoice}`
    })
  }

  // MARK: Ecash

  ecashReceive() {
    if (this.isWorking('ecashReceive')) return
    // The SDK calls take no signal, as the iOS demo's ignore cancellation: an attach drops this
    // run's result, but notes already handed to the federation may still be redeemed, and once
    // they are, the field must clear.
    this.run('ecashReceive', async () => {
      const ecash = this.federation?.ecash()
      if (!ecash) return 'this federation has no mint module'
      const text = this.state.ecashNotes.trim()
      if (!text) return 'paste ecash notes first'
      const notes = Notes.parse(text)
      const operation = await ecash.receive(notes)
      const state = await operation.awaitFinal()
      let outcome: string
      switch (state.tag) {
        case EcashReceiveState_Tags.Done:
          // Spent now, so a second Redeem cannot report it as invalid.
          this.set({ ecashNotes: '' })
          outcome = `Redeemed ${formatMsats(notes.value())}`
          break
        case EcashReceiveState_Tags.Failed:
          outcome = `Not redeemed: ${(state.inner as { reason: string }).reason}`
          break
        default:
          outcome = `Ended in ${state.tag}`
      }
      return `${outcome}\n\noperation ${operation.id()}`
    })
  }

  ecashQuoteAmount() {
    if (this.isWorking('ecashSend')) return
    this.set({ ecashQuote: undefined })
    this.run('ecashSend', async (signal) => {
      const ecash = this.federation?.ecash()
      if (!ecash) return 'this federation has no mint module'
      const msats = parseAmount(this.state.ecashSendAmount)
      if (msats === undefined) return 'enter an amount in msats'
      const quote = await ecash.quote(msats, { signal })
      if (signal.aborted) return 'superseded by a newer quote'
      if (msats !== parseAmount(this.state.ecashSendAmount)) {
        return 'the amount changed while quoting, so quote again'
      }
      this.set({ ecashQuote: quote })
      return 'quoted'
    })
  }

  /**
   * Sends ecash, from the held quote when there is one. Refused while a send is committing:
   * this rail quotes and sends in one block, so two taps would be two real spends.
   */
  ecashSend(): boolean {
    if (this.state.isSendingEcash || this.isAttaching) return false
    const held = this.state.ecashQuote
    this.stopWatching('ecashSend')
    this.lastEcashSend = undefined
    this.set({
      isSendingEcash: true,
      ecashQuote: undefined,
      ecashSendState: undefined,
      lastNotes: undefined,
    })
    this.run('ecashSend', async (signal) => {
      try {
        const ecash = this.federation?.ecash()
        if (!ecash) return 'this federation has no mint module'
        const msats = parseAmount(this.state.ecashSendAmount)
        if (msats === undefined) return 'enter an amount in msats'
        const quote = held ?? (await ecash.quote(msats, { signal }))
        // Checked before anything commits: a switch during the quote must not spend from the
        // federation the user has just left.
        if (signal.aborted) return 'federation changed, so quote again'
        // No signal from here on: the send commits, and its notes are the only record of where
        // that value went. Dropping them because the run was superseded loses funds outright.
        const sent = await ecash.send(quote)
        this.lastEcashSend = sent.operation
        this.set({ lastNotes: sent.notes })
        this.watch('ecashSend', sent.operation.updates(), (state) =>
          this.set({ ecashSendState: state }),
        )
        return `notes ${formatMsats(quote.notesValue())} + fee ${formatMsats(quote.fee())}`
      } finally {
        this.set({ isSendingEcash: false })
      }
    })
    return true
  }

  /** Asks to take the last ecash send back. A request, not a guarantee. */
  reclaimEcash() {
    const operation = this.lastEcashSend
    if (this.isWorking('ecashReclaim') || !operation) return
    this.run('ecashReclaim', async (signal) => {
      await operation.requestCancel({ signal })
      return 'reclaim requested'
    })
  }

  // MARK: Lightning out

  lnQuoteInvoice() {
    if (this.isWorking('lnPay')) return
    this.set({ lnQuote: undefined })
    this.run('lnPay', async (signal) => {
      const lightning = this.federation?.lightning()
      if (!lightning) return 'this federation has no lightning module'
      const invoice = this.state.lnInvoice.trim()
      if (!invoice) return 'paste an invoice first'
      const quote = await lightning.quote(invoice, { signal })
      if (signal.aborted) return 'superseded by a newer quote'
      if (invoice !== this.state.lnInvoice.trim()) {
        return 'the invoice changed while quoting, so quote again'
      }
      this.set({ lnQuote: quote })
      return 'quoted'
    })
  }

  lnPay(): boolean {
    const quote = this.state.lnQuote
    const lightning = this.federation?.lightning()
    if (this.isAttaching || !quote || !lightning) return false
    this.stopWatching('lnPay')
    this.set({
      lnQuote: undefined,
      isPayingLightning: true,
      lnInvoice: '',
      lnSendState: undefined,
    })
    this.run('lnPay', async () => {
      try {
        // No signal: once submitted the payment is under way, and its operation is how it is
        // followed.
        const operation = await lightning.send(quote)
        this.watch('lnPay', operation.updates(), (state) =>
          this.set({ lnSendState: state }),
        )
        return `operation ${operation.id()}`
      } finally {
        this.set({ isPayingLightning: false })
      }
    })
    return true
  }

  // MARK: On-chain

  /**
   * Shows a deposit address, then follows the first deposit found to it. The subscription is
   * opened before the address is requested: it yields only deposits found after it opened.
   */
  deposit() {
    if (this.isWorking('deposit')) return
    this.run('deposit', async (signal) => {
      const onchain = this.federation?.onchain()
      if (!onchain) return 'this federation has no wallet module'
      const deposits = await onchain.deposits({ signal })
      const address = await onchain.receive({ signal })
      if (signal.aborted) return 'superseded by a newer request'
      // Stopped only now, so a request that fails leaves the address on screen still followed.
      this.stopWatching('deposit')
      this.set({
        lastAddress: address,
        depositFound: false,
        depositState: undefined,
      })
      this.watchNextDeposit(deposits, address)
      return `Send bitcoin to:\n${address}`
    })
  }

  onchainQuoteAddress() {
    if (this.isWorking('onchainSend')) return
    this.set({ onchainQuote: undefined })
    this.run('onchainSend', async (signal) => {
      const onchain = this.federation?.onchain()
      if (!onchain) return 'this federation has no wallet module'
      const sats = parseAmount(this.state.onchainAmount)
      if (sats === undefined) return 'enter an amount in sats'
      const address = this.state.onchainAddress.trim()
      if (!address) return 'enter a destination address'
      const quote = await onchain.quote(address, sats, { signal })
      if (signal.aborted) return 'superseded by a newer quote'
      if (
        address !== this.state.onchainAddress.trim() ||
        sats !== parseAmount(this.state.onchainAmount)
      ) {
        return 'the destination changed while quoting, so quote again'
      }
      this.set({ onchainQuote: quote })
      return 'quoted'
    })
  }

  onchainSend(): boolean {
    const quote = this.state.onchainQuote
    const onchain = this.federation?.onchain()
    if (this.isAttaching || !quote || !onchain) return false
    this.stopWatching('onchainSend')
    this.set({
      onchainQuote: undefined,
      isSendingOnchain: true,
      onchainSendState: undefined,
    })
    this.run('onchainSend', async () => {
      try {
        const operation = await onchain.send(quote)
        this.watch('onchainSend', operation.updates(), (state) =>
          this.set({ onchainSendState: state }),
        )
        return `operation ${operation.id()}`
      } finally {
        this.set({ isSendingOnchain: false })
      }
    })
    return true
  }

  /** Drops every held quote: the user declined it, so nothing may execute it. */
  discardQuotes() {
    this.set({
      lnQuote: undefined,
      onchainQuote: undefined,
      ecashQuote: undefined,
    })
  }

  // MARK: Recovery

  /**
   * Follows the attached federation's recovery again: the attempt that is running, or a new one
   * if the last attempt stopped. Because of the second case, the recovery screen opens with
   * `followRecovery` and leaves this to the user's tap.
   */
  resumeRecovery() {
    const sdk = this.session?.sdk
    const id = this.state.federationId
    if (this.isWorking('recovery') || !sdk || !id) return
    this.run('recovery', async (signal) => {
      const handle = await sdk.resumeRecovery(id, { signal })
      if (signal.aborted) return 'superseded by a newer resume'
      this.watch('recovery', handle.progress.updates(), (state) =>
        this.recoveryProgressed(state),
      )
      return `recovery ${handle.progress.id()}`
    })
  }

  /**
   * Reads where the recovery stands without starting anything, and follows it only if it is
   * running. A stopped attempt is left stopped: resuming it starts a new attempt, which is the
   * user's call, not the screen's.
   */
  followRecovery() {
    const sdk = this.session?.sdk
    const id = this.state.federationId
    if (!sdk || !id) return
    sdk.recoveryStatus(id).then(
      (state) => {
        if (this.state.federationId !== id || !state || this.followingRecovery)
          return
        this.recoveryProgressed(state)
        if (state.tag === RecoveryState_Tags.Running) this.resumeRecovery()
      },
      (error) => {
        if (this.state.federationId === id) {
          this.fail('recovery', `couldn't read the recovery: ${reason(error)}`)
        }
      },
    )
  }

  /** Something is already following the recovery, or about to. */
  private get followingRecovery(): boolean {
    if (this.isWorking('recovery')) return true
    if (!this.watchers.has('recovery') || this.state.watchFailures.recovery)
      return false
    const tag = this.state.recoveryState?.tag
    return tag === undefined || tag === RecoveryState_Tags.Running
  }

  /** A completed recovery also moved the federation's status on, so that is read with it. */
  private recoveryProgressed(state: RecoveryState) {
    this.set({ recoveryState: state })
    const id = this.state.federationId
    if (state.tag === RecoveryState_Tags.Done && this.session && id) {
      this.set({ federationStatus: statusOf(this.session.sdk, id) })
    }
  }

  // MARK: Activity

  /** One operation of the attached federation, for a screen that follows it. */
  operation(id: string) {
    return this.federation?.operation(id)
  }

  activity() {
    this.run('activity', async (signal) => {
      const federation = this.federation
      if (!federation) return 'join a federation first'
      const page = await federation.activity(undefined, activityPageSize, {
        signal,
      })
      if (signal.aborted) return 'superseded by a newer load'
      this.set({ activityItems: page.items, activityNext: page.next })
      return page.items.length
        ? `loaded ${page.items.length}`
        : 'no activity yet'
    })
  }

  moreActivity() {
    const cursor = this.state.activityNext
    if (cursor === undefined) return
    this.run('activity', async (signal) => {
      const federation = this.federation
      if (!federation) return 'join a federation first'
      const page = await federation.activity(cursor, activityPageSize, {
        signal,
      })
      if (signal.aborted || cursor !== this.state.activityNext)
        return 'superseded by a newer load'
      this.set({
        activityItems: [...this.state.activityItems, ...page.items],
        activityNext: page.next,
      })
      return `loaded ${page.items.length} more`
    })
  }

  /**
   * Reloads the newest page when the balance moves, since the SDK has no activity stream. Merged
   * where it joins the rows already loaded; otherwise it replaces them with its own cursor.
   */
  private refreshRecentActivity() {
    const federation = this.federation
    if (!federation) return
    this.activityRefresh?.abort()
    const controller = new AbortController()
    this.activityRefresh = controller
    federation
      .activity(undefined, activityPageSize, { signal: controller.signal })
      .then((page) => {
        if (controller.signal.aborted || this.federation !== federation) return
        const fresh = new Set(page.items.map((item) => item.operationId))
        const items = this.state.activityItems
        if (page.next === undefined) {
          this.set({ activityItems: page.items, activityNext: undefined })
        } else if (items.some((item) => fresh.has(item.operationId))) {
          this.set({
            activityItems: [
              ...page.items,
              ...items.filter((i) => !fresh.has(i.operationId)),
            ],
          })
        } else {
          this.set({ activityItems: page.items, activityNext: page.next })
        }
      })
      .catch(() => {})
  }

  // MARK: Attaching

  /**
   * Makes `joined` the current federation and follows its balance. `remember` makes it the one
   * a relaunch comes back to.
   */
  private attach(joined: FederationLike, remember = true) {
    if (remember) {
      this.lastFederationId = joined.id()
      writeLastFederationId(joined.id())
    }
    this.federation = joined
    this.balanceWatch?.abort()
    // Every open watcher belongs to the federation being replaced.
    for (const controller of this.watchers.values()) controller.abort()
    this.watchers.clear()
    this.activityRefresh?.abort()
    this.lastEcashSend = undefined

    const results = { ...this.state.results }
    const failures = { ...this.state.failures }
    // Cancelling is what stops an action still in flight against the federation being replaced
    // from repopulating these on its way out. `federationScoped` excludes `join`, `wallet` and
    // `switching`: `attach` runs inside one of those.
    for (const section of federationScoped) {
      this.stopRun(section)
      delete results[section]
      delete failures[section]
    }

    this.set({
      attachments: this.state.attachments + 1,
      hasFederation: true,
      lnQuote: undefined,
      onchainQuote: undefined,
      ecashQuote: undefined,
      lnSendState: undefined,
      onchainSendState: undefined,
      ecashSendState: undefined,
      recoveryState: undefined,
      balanceMsats: undefined,
      balanceFailure: undefined,
      // Read synchronously, so these always describe the federation this call attached.
      federationName: joined.name(),
      federationId: joined.id(),
      federationInvite: joined.inviteCode().display(),
      federationNetwork: joined.network(),
      capabilities: joined.capabilities(),
      federationStatus: this.session
        ? statusOf(this.session.sdk, joined.id())
        : undefined,
      activityItems: [],
      activityNext: undefined,
      // All of this names values from the federation being replaced. `lastNotes` matters most:
      // ecash is a bearer token, and those notes are drawn on a different mint.
      lastInvoice: undefined,
      lastNotes: undefined,
      lastAddress: undefined,
      lnReceiveState: undefined,
      lnReceiveExpiresAt: undefined,
      lnReceiveInvoiceMsats: undefined,
      depositFound: false,
      depositState: undefined,
      watchFailures: {},
      results,
      failures,
    })
    this.followBalance(joined)
  }

  /**
   * Keeps `storedFederations` and `federationStatus` live for the life of `sdk`. Each update, a
   * join included, is the cue to read the whole list again: `storedFederations()` is in-memory
   * and synchronous, and a list read whole cannot keep a federation whose `forgotten` update was
   * missed while resubscribing or catching up.
   */
  private followFederationStatuses(sdk: SdkLike) {
    this.statusWatch?.abort()
    const controller = new AbortController()
    this.statusWatch = controller
    this.set({ storedFederations: sdk.storedFederations() })
    ;(async () => {
      while (!controller.signal.aborted) {
        const updates = sdk.federationStatusUpdates()
        try {
          for (;;) {
            const info = await updates.next({ signal: controller.signal })
            if (controller.signal.aborted) return
            const list = sdk.storedFederations()
            if (!sameList(list, this.state.storedFederations))
              this.set({ storedFederations: list })
            if (info.id === this.state.federationId) {
              // From the list read now rather than from `info`: an update sent before a reopen
              // can arrive after it. `info` only for an id the list no longer holds.
              this.set({
                federationStatus:
                  list.find((f) => f.id === info.id)?.status ?? info.status,
              })
            }
          }
        } catch (error) {
          if (controller.signal.aborted || isAbort(error)) return
          // The instance has shut down: the last statuses known are the ones worth keeping.
          if (sdkError(error)?.code === ErrorCode.FederationClosed) return
          // A storage or internal failure. A new subscription replays every federation's state.
          await new Promise<void>((resolve) => setTimeout(resolve, 5000))
        }
      }
    })()
  }

  /**
   * Keeps `balanceMsats` live from `joined`'s balance updates. A failure leaves the last amount
   * in place and says so in `balanceFailure`.
   */
  private followBalance(joined: FederationLike) {
    this.balanceWatch?.abort()
    const controller = new AbortController()
    this.balanceWatch = controller
    const { signal } = controller
    ;(async () => {
      try {
        const initial = await joined.balance({ signal })
        if (signal.aborted) return
        this.set({ balanceMsats: initial, balanceFailure: undefined })
        const updates = joined.balanceUpdates()
        for (;;) {
          const next = await updates.next({ signal })
          if (signal.aborted) return
          this.set({ balanceMsats: next })
          this.refreshRecentActivity()
        }
      } catch (error) {
        if (signal.aborted || isAbort(error)) return
        this.set({ balanceFailure: reason(error) })
      }
    })()
  }

  /** Reads the balance again, and restarts a balance subscription that had failed. */
  refreshBalance() {
    this.run('wallet', async (signal) => {
      const federation = this.federation
      if (!federation) return 'join a federation first'
      const amount = await federation.balance({ signal })
      if (signal.aborted || this.federation !== federation)
        return 'federation changed, so refresh again'
      this.set({ balanceMsats: amount })
      if (this.state.balanceFailure) this.followBalance(federation)
      return 'balance refreshed'
    })
  }

  // MARK: Runs and watchers

  private fail(section: Section, reasonText: string) {
    this.setIn('results', section, reasonText)
    this.setIn('failures', section, reasonText)
    if (!expiringFailures.has(section)) return
    // Not once another run has started, or another failure replaced this one.
    const generation = this.runGenerations.get(section) ?? 0
    this.expire(
      () =>
        (this.runGenerations.get(section) ?? 0) === generation &&
        this.state.failures[section] === reasonText,
      () => {
        this.setIn('failures', section, undefined)
        if (this.state.results[section] === reasonText)
          this.setIn('results', section, undefined)
      },
    )
  }

  /** Runs `clear` after `failureLifetime`, if `current` says it is still the one on screen. */
  private expire(current: () => boolean, clear: () => void) {
    setTimeout(() => {
      if (current()) clear()
    }, failureLifetime)
  }

  /** Drops a section's failure now, for a screen starting a new visit. */
  clearFailure(section: Section) {
    this.setIn('failures', section, undefined)
  }

  private stopRun(section: Section) {
    this.runs.get(section)?.abort()
    this.runs.delete(section)
    this.runGenerations.set(
      section,
      (this.runGenerations.get(section) ?? 0) + 1,
    )
    if (this.state.working.has(section)) {
      const working = new Set(this.state.working)
      working.delete(section)
      this.set({ working })
    }
  }

  /**
   * Runs `block` as `section`'s action, cancelling the section's previous run. The block reads
   * `signal.aborted` after every await, as the iOS model reads `isSuperseded`: it guards the
   * writes a block makes on its way, which the result-text check below cannot.
   */
  private run(
    section: Section,
    block: (signal: AbortSignal) => Promise<string>,
  ) {
    this.stopRun(section)
    const generation = this.runGenerations.get(section) ?? 0
    const controller = new AbortController()
    this.runs.set(section, controller)
    this.set({ working: new Set(this.state.working).add(section) })
    this.setIn('results', section, workingText)
    this.setIn('failures', section, undefined)
    block(controller.signal)
      .then(
        (output) => {
          if (!controller.signal.aborted) this.setIn('results', section, output)
        },
        (error) => {
          if (!controller.signal.aborted && !isAbort(error))
            this.fail(section, describe(error))
        },
      )
      .finally(() => {
        if (
          this.runGenerations.get(section) === generation &&
          this.state.working.has(section)
        ) {
          const working = new Set(this.state.working)
          working.delete(section)
          this.set({ working })
        }
      })
  }

  /** Stops following `section`'s previous operation, without cancelling the operation. */
  private stopWatching(section: Section) {
    this.watchers.get(section)?.abort()
    this.watchers.delete(section)
    this.setIn('watchFailures', section, undefined)
  }

  /**
   * Follows an operation's state changes into `onState` until the subscription ends: `next()`
   * resolves `undefined` once the state is final. The first update is the current state.
   */
  private watch<S>(
    section: Section,
    updates: Updates<S>,
    onState: (state: S) => void,
  ) {
    this.watchers.get(section)?.abort()
    this.setIn('watchFailures', section, undefined)
    const controller = new AbortController()
    this.watchers.set(section, controller)
    ;(async () => {
      try {
        for (;;) {
          const state = await updates.next({ signal: controller.signal })
          if (controller.signal.aborted || state === undefined) return
          onState(state)
        }
      } catch (error) {
        if (!controller.signal.aborted && !isAbort(error)) {
          this.setIn('watchFailures', section, reason(error))
        }
      }
    })()
  }

  /**
   * Waits for the next deposit to `address`, then follows its operation. The wallet yields
   * deposits to any address it has handed out, so the ones to other addresses are skipped.
   */
  private watchNextDeposit(deposits: OnchainDepositsLike, address: string) {
    const controller = new AbortController()
    this.watchers.set('deposit', controller)
    const { signal } = controller
    ;(async () => {
      try {
        let found = await deposits.next({ signal })
        while (
          !signal.aborted &&
          (await found.details({ signal })).address !== address
        ) {
          found = await deposits.next({ signal })
        }
        if (signal.aborted) return
        this.set({ depositFound: true })
        // `watch` replaces this watcher with the deposit's own: the hand-over.
        this.watch('deposit', found.updates(), (state) =>
          this.set({ depositState: state }),
        )
      } catch (error) {
        // The address stays on screen above the error: it may already have been shown to a
        // payer.
        if (!signal.aborted && !isAbort(error))
          this.setIn('watchFailures', 'deposit', reason(error))
      }
    })()
  }
}

/** Whole, non-negative amounts only, as BigInt so a full u64 stays exact. */
export function parseAmount(text: string): bigint | undefined {
  const trimmed = text.trim()
  if (!/^\d+$/.test(trimmed)) return undefined
  const value = BigInt(trimmed)
  // The SDK takes a u64, as Swift's `UInt64(_:)` does: anything larger is no amount at all.
  return value <= maxU64 ? value : undefined
}

const maxU64 = 2n ** 64n - 1n

/**
 * A whole, positive number of sats in msats, or `undefined` when there is none or it does not
 * fit a u64 once multiplied, as iOS's `multipliedReportingOverflow`.
 */
export function satsToMsats(text: string): bigint | undefined {
  const sats = parseAmount(text)
  if (sats === undefined || sats === 0n) return undefined
  const msats = sats * 1000n
  return msats <= maxU64 ? msats : undefined
}

/** "1012 sat", or "1012 sat (1012250 msat)" when there is a remainder, as on iOS. */
function formatMsats(msats: bigint): string {
  const sats = msats / 1000n
  return msats % 1000n === 0n ? `${sats} sat` : `${sats} sat (${msats} msat)`
}

/**
 * A federation's status, or none when it cannot be read, as iOS's `try?`: the status is shown
 * as unknown, and the attach or recovery it is read for carries on.
 */
function statusOf(sdk: SdkLike, id: string): FederationStatus | undefined {
  try {
    return sdk.federationStatus(id)
  } catch {
    return undefined
  }
}

function sameList(a: FederationInfo[], b: FederationInfo[]): boolean {
  if (a.length !== b.length) return false
  return a.every((info, i) => {
    const other = b[i]
    return (
      other !== undefined &&
      info.id === other.id &&
      info.name === other.name &&
      JSON.stringify(info.status, bigintSafe) ===
        JSON.stringify(other.status, bigintSafe)
    )
  })
}

function bigintSafe(_key: string, value: unknown) {
  return typeof value === 'bigint' ? value.toString() : value
}

/** The one model for the app, shared by every screen. */
export const model = new DemoModel()

/** The model's current state, re-rendering the caller on every change. */
export function useDemo(): DemoState {
  return useSyncExternalStore(model.subscribe, model.getState)
}

/**
 * Calls `close` once another federation is attached, for a screen about the one attached when it
 * opened. Any attach counts, even one that reattaches the same federation: it replaces the
 * handles the screen read through, and clears the quotes and failures it shows.
 */
export function useCloseOnAttach(close: () => void) {
  const { attachments } = useDemo()
  const opened = useRef(attachments)
  useEffect(() => {
    if (attachments !== opened.current) close()
  }, [attachments, close])
}
