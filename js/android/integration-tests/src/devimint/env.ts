import { execFileSync } from 'child_process'

import { NETWORK_TIMEOUT } from '../configs/appium/AppiumTestBase'

// devimint exports ready-to-run command lines for the services it starts
// (FM_BTC_CLIENT, FM_LNCLI, FM_GWCLI_LDK, ...) into the process it execs.
// These helpers run them from the host, where the runner lives.

/** Reads a variable devimint exports, failing loudly when it is missing. */
export function requireEnv(name: string): string {
  const value = process.env[name]
  if (!value) {
    throw new Error(
      `${name} is not set. Is this running under scripts/setup_test_shell.sh, ` +
        'which execs the run inside devimint?',
    )
  }
  return value
}

/** Arguments come off the screen or out of another tool, so they are checked
 * rather than trusted — nothing here goes through a shell, but a stray
 * newline or space would still split one argument into two. */
const SAFE_ARG = /^[A-Za-z0-9._:=/-]+$/

/**
 * Runs the command line in `envVar` with `args` appended and returns its
 * trimmed stdout. The command line is split on whitespace, the same way the
 * Rust integration tests split it, and run without a shell.
 */
export function runCli(
  envVar: string,
  args: readonly string[],
  timeout = NETWORK_TIMEOUT,
): string {
  const [program, ...base] = requireEnv(envVar).trim().split(/\s+/)
  for (const arg of args) {
    if (!SAFE_ARG.test(arg)) {
      throw new Error(`Refusing to pass "${arg}" to ${envVar}`)
    }
  }
  try {
    return execFileSync(program, [...base, ...args], {
      stdio: ['ignore', 'pipe', 'pipe'],
      timeout,
      maxBuffer: 16 * 1024 * 1024,
    })
      .toString()
      .trim()
  } catch (error) {
    const err = error as { stderr?: Buffer; message: string }
    throw new Error(
      `${envVar} ${args.join(' ')} failed: ${err.stderr?.toString().trim() || err.message}`,
    )
  }
}

export const sleep = (ms: number) =>
  new Promise((resolve) => setTimeout(resolve, ms))
