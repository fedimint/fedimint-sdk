import {
  openSdk,
  type FederationLike,
  type Proxied,
  type SdkSession,
} from '@fedimint/web-sdk'
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react'

/**
 * What the page has open: the SDK session, and the federation its sections work on. `version`
 * ticks whenever the list of federations may have changed, so the list reloads. `fromSeed` says
 * the session was opened from seed words, so its federations are recovered rather than joined.
 * `deselect(id)` drops the federation in use only if it is still that one.
 */
interface SdkState {
  session: SdkSession | undefined
  fromSeed: boolean
  federation: Proxied<FederationLike> | undefined
  federationId: string | undefined
  faucetUrl: string
  version: number
  open(storage: string, mnemonic?: string[]): Promise<void>
  close(): Promise<void>
  selectFederation(
    federation: Proxied<FederationLike> | undefined,
  ): Promise<void>
  deselect(id: string): void
  setFaucetUrl(url: string): void
  federationsChanged(): void
}

const SdkContext = createContext<SdkState | undefined>(undefined)

export function SdkProvider({
  children,
  initialFaucetUrl,
}: {
  children: ReactNode
  initialFaucetUrl: string
}) {
  const [session, setSession] = useState<SdkSession>()
  const [fromSeed, setFromSeed] = useState(false)
  // The federation and its id change together.
  const [selected, setSelected] = useState<{
    federation: Proxied<FederationLike>
    id: string
  }>()
  const selecting = useRef(0)
  const [faucetUrl, setFaucetUrl] = useState(initialFaucetUrl)
  const [version, setVersion] = useState(0)

  // The last call wins: one that ends after a later one has no effect.
  const selectFederation = useCallback(
    async (next: Proxied<FederationLike> | undefined) => {
      const call = ++selecting.current
      const id = next && (await next.id())
      if (call === selecting.current)
        setSelected(next && { federation: next, id: id! })
    },
    [],
  )

  const deselect = useCallback(
    (id: string) =>
      setSelected((current) => (current?.id === id ? undefined : current)),
    [],
  )

  const close = useCallback(async () => {
    const closing = session
    await selectFederation(undefined)
    setSession(undefined)
    await closing?.close()
  }, [session, selectFederation])

  const open = useCallback(
    async (storage: string, mnemonic?: string[]) => {
      await close()
      // The worker and the wasm module load here, on the first call into the SDK.
      const opened = await openSdk({ storage, mnemonic })
      setFromSeed(mnemonic !== undefined)
      setSession(opened)
      const [first] = await opened.sdk.federations()
      await selectFederation(first)
    },
    [close, selectFederation],
  )

  const federationsChanged = useCallback(() => setVersion((v) => v + 1), [])

  const value = useMemo<SdkState>(
    () => ({
      session,
      fromSeed,
      federation: selected?.federation,
      federationId: selected?.id,
      faucetUrl,
      version,
      open,
      close,
      selectFederation,
      deselect,
      setFaucetUrl,
      federationsChanged,
    }),
    [
      session,
      fromSeed,
      selected,
      faucetUrl,
      version,
      open,
      close,
      selectFederation,
      deselect,
      federationsChanged,
    ],
  )

  return <SdkContext.Provider value={value}>{children}</SdkContext.Provider>
}

export function useSdk(): SdkState {
  const state = useContext(SdkContext)
  if (!state) throw new Error('useSdk needs an SdkProvider above it')
  return state
}

/** The open session, for a section that only renders once the wallet is open. */
export function useSession(): SdkSession {
  const { session } = useSdk()
  if (!session) throw new Error('open the wallet first')
  return session
}

/** The federation in use, for a section that only renders once one is selected. */
export function useFederation(): Proxied<FederationLike> {
  const { federation } = useSdk()
  if (!federation) throw new Error('join or pick a federation first')
  return federation
}
