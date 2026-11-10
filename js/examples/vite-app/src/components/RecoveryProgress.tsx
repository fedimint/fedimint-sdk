import {
  RecoveryState_Tags,
  type Proxied,
  type RecoveryOperationLike,
  type SdkSession,
} from '@fedimint/web-sdk'
import { useEffect, useState } from 'react'
import { errorText, variant } from '../format'
import { useSdk, useSession } from '../sdk'

/** A recovery's state, as it crosses from the worker. */
type RecoveryState = NonNullable<
  Awaited<ReturnType<SdkSession['sdk']['recoveryStatus']>>
>

/**
 * Follows the recovery of the federation in use, when it has one: `sdk.recoveryStatus`,
 * `sdk.resumeRecovery` and the recovery operation's `updates`. A recovery carries on across
 * restarts, and `resumeRecovery` hands back the attempt that is running.
 */
export function RecoveryProgress() {
  const session = useSession()
  const { federationId, federationsChanged } = useSdk()
  const [state, setState] = useState<RecoveryState>()
  const [error, setError] = useState('')

  useEffect(() => {
    setState(undefined)
    setError('')
    if (!federationId) return
    const controller = new AbortController()
    const signal = controller.signal
    void (async () => {
      try {
        // `undefined` for a federation that was joined rather than recovered.
        const status = await session.sdk.recoveryStatus(federationId)
        if (!status || status.tag === RecoveryState_Tags.Done) return
        setState(status)
        const { progress } = await session.sdk.resumeRecovery(federationId)
        await follow(progress, signal, setState)
        federationsChanged()
      } catch (e) {
        if (!signal.aborted) setError(errorText(e))
      }
    })()
    return () => controller.abort()
  }, [session, federationId, federationsChanged])

  if (!state && !error) return null

  const progress =
    state?.tag === RecoveryState_Tags.Running ? state.inner.progress : undefined
  const percent =
    progress && progress.total > 0
      ? Math.round((progress.complete / progress.total) * 100)
      : 0
  const done = state?.tag === RecoveryState_Tags.Done

  return (
    <div className="section">
      <h3>Recovery Progress</h3>
      {state && (
        <>
          <div className="row">
            <span className={`state${done ? ' final' : ''}`}>
              {variant(state)}
            </span>
          </div>
          <div className={`progress-bar${done ? ' done' : ''}`}>
            <div style={{ width: `${done ? 100 : Math.min(percent, 100)}%` }} />
          </div>
          {progress && (
            <div className="muted">
              {progress.complete.toLocaleString()} /{' '}
              {progress.total.toLocaleString()} ({percent}%)
            </div>
          )}
        </>
      )}
      {error && <div className="error">{error}</div>}
    </div>
  )
}

async function follow(
  progress: Proxied<RecoveryOperationLike>,
  signal: AbortSignal,
  onState: (state: RecoveryState) => void,
): Promise<void> {
  const updates = await progress.updates()
  for (;;) {
    const next = await updates.next({ signal })
    if (next === undefined) break
    onState(next)
  }
  onState(await progress.state())
}
