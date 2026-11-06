import fs from 'fs'

import { Shape } from '../env/shape'

/**
 * The DNS check run-android-e2e.sh made, over the SDK's own log for the run
 * (`SDK_LOG_FILE`, captured by the target's provisioning script).
 *
 * iroh's resolver reads the device's DNS servers and, when it cannot, logs a
 * warning and falls back to public DNS while every test still passes. So the
 * warning fails the run. When the lightning test ran on v1 it dialled
 * devimint's iroh gateway, which builds the resolver, so the trace line
 * listing the servers it read is required too; on v2 the lnv2 gateway list is
 * pinned to the plain HTTP LND gateway and no resolver need be built.
 */
export function checkDnsConfig(
  logFile: string,
  shape: Shape,
  ran: readonly string[],
  requireServersLine: boolean,
): string[] {
  const log = fs.existsSync(logFile) ? fs.readFileSync(logFile, 'utf8') : ''
  if (log.includes('failed to read system DNS configuration')) {
    return [
      `The SDK could not read the device's DNS configuration and fell back to public DNS (see ${logFile})`,
    ]
  }
  if (log.includes('Got DNS servers')) return []
  if (!requireServersLine || shape !== 'v1' || !ran.includes('lightning')) {
    return []
  }
  return [
    "The lightning test ran, but no DNS resolver read the device's DNS configuration " +
      `(no "Got DNS servers" in ${logFile}), so the platform DNS path went untested.`,
  ]
}
