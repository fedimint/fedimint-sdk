import { ConsoleBackend } from '../backends/types'
import { Method } from '../protocol/methods'

export type TargetName = 'android' | 'ios' | 'web'

/**
 * One platform the suite runs on: how to reach its harness and how to
 * restart, reset and inspect it. The device or browser is already provisioned
 * by scripts/e2e/<target>.sh when the runner starts.
 */
export interface Target {
  readonly name: TargetName
  /**
   * Methods or tests this target is known not to support yet, each with the
   * reason. Tests needing one are reported as skipped; anything else missing
   * from the harness fails the run, so coverage is never lost silently.
   */
  readonly knownUnsupported: Readonly<Partial<Record<Method | string, string>>>
  /** Opens the session and launches the harness from a fresh install. */
  start(): Promise<ConsoleBackend>
  /** Kills the harness process, keeping its data, and launches it again. */
  restart(): Promise<void>
  /** Wipes the harness's data and launches it fresh. */
  reset(): Promise<void>
  /** Saves what explains a failure into the artifacts directory. */
  captureFailure(test: string): Promise<void>
  /** Checks made once the tests are done (the SDK's own logs, say); returns
   * one line per problem found. */
  afterRun(ran: readonly string[]): Promise<string[]>
  stop(): Promise<void>
}
