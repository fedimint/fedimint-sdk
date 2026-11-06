/** Longer budget for calls that reach a real federation over the network. */
export const NETWORK_TIMEOUT = 60_000

/** The mutinynet federation the demos pre-fill: a real invite code for the
 * tests that parse one without joining. */
export const MUTINYNET_INVITE_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

/** Where a run writes screenshots, UI trees and logs. */
export function artifactsDir(): string {
  return process.env.ARTIFACTS_DIR || `${process.cwd()}/.artifacts/local`
}
