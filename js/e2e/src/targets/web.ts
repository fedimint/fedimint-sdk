/* eslint-disable no-console */
import fs from 'fs'

import { Browser, BrowserContext, chromium, Page } from 'playwright'

import { PlaywrightConsoleBackend } from '../backends/playwright'
import { ConsoleBackend } from '../backends/types'
import { artifactsDir } from '../env/constants'

import { Target } from './types'

function requireEnv(name: string): string {
  const value = process.env[name]
  if (!value) {
    throw new Error(`${name} is not set; scripts/e2e/web.sh exports it`)
  }
  return value
}

/**
 * The web harness (js/web/sdk-harness) in Chromium, driven through
 * Playwright. scripts/e2e/web.sh serves the built page and exports its URL.
 * The browser runs on the host, so devimint's 127.0.0.1 needs no forwarding.
 *
 * A browser context is the install: its origin-private storage is the SDK's.
 * A restart reopens the page in the same context, keeping that storage; a
 * reset opens a new context, with none.
 *
 * The browser is Playwright's own, or the one `CHROMIUM_EXECUTABLE` or
 * `PLAYWRIGHT_CHANNEL` names (`chrome` for an installed Google Chrome).
 */
export class WebTarget implements Target {
  readonly name = 'web'
  readonly knownUnsupported = {
    // uniffi's wasm32 `Result` return path has no `handle_failed_lift`
    // (uniffi_core/src/ffi_converter_impls.rs), so a custom-type argument
    // that fails to parse, such as a malformed invoice, throws an internal
    // error instead of the SDK's INVALID_INPUT. Remove once the SDK maps it.
    errors:
      'a malformed invoice throws an internal error on web, not INVALID_INPUT (uniffi wasm handle_failed_lift)',
    // fedimint-lnv2-client checks expiry with `Bolt11Invoice::is_expired`,
    // which calls `SystemTime::now` and traps on wasm32. Remove once the
    // SDK's fedimint pin includes fedimint/fedimint#9424.
    ...(process.env.FM_SDK_SHAPE === 'v2' && {
      lightningSend:
        'lnv2 send panics on wasm: fedimint-lnv2-client calls SystemTime::now (fedimint/fedimint#9424)',
    }),
  }

  private readonly url = requireEnv('HARNESS_URL')
  private readonly logFile = process.env.SDK_LOG_FILE
  private browser: Browser | null = null
  private context: BrowserContext | null = null
  private page: Page | null = null
  private readonly backend = new PlaywrightConsoleBackend(() => {
    if (!this.page) throw new Error('The harness page is not open')
    return this.page
  })

  async start(): Promise<ConsoleBackend> {
    this.browser = await chromium.launch({
      executablePath: process.env.CHROMIUM_EXECUTABLE || undefined,
      channel: process.env.PLAYWRIGHT_CHANNEL || undefined,
      headless: !process.env.HEADED,
    })
    await this.reset()
    return this.backend
  }

  async restart(): Promise<void> {
    await this.page?.close()
    await this.openPage()
  }

  async reset(): Promise<void> {
    await this.context?.close()
    if (!this.browser) throw new Error('The browser is not started')
    this.context = await this.browser.newContext()
    await this.openPage()
  }

  async captureFailure(test: string): Promise<void> {
    try {
      await this.backend.captureUi(
        artifactsDir(),
        `${test}-failure-${Date.now()}`,
      )
    } catch (error) {
      console.error(`Could not capture the page: ${(error as Error).message}`)
    }
  }

  async afterRun(): Promise<string[]> {
    return []
  }

  async stop(): Promise<void> {
    await this.browser?.close().catch(() => {})
    this.browser = null
  }

  /** Opens the harness in the current context, logging its console (the SDK
   * worker's included) to SDK_LOG_FILE. */
  private async openPage(): Promise<void> {
    if (!this.context) throw new Error('No browser context')
    const page = await this.context.newPage()
    const log = (source: string, text: string) => {
      if (this.logFile) {
        fs.appendFileSync(
          this.logFile,
          `${new Date().toISOString()} ${source} ${text}\n`,
        )
      }
    }
    // A dedicated worker's console reaches the page's `console` event.
    page.on('console', (m) => log(`console:${m.type()}`, m.text()))
    page.on('pageerror', (e) => log('pageerror', e.message))
    page.on('worker', (w) => log('worker', `started ${w.url()}`))
    this.page = page
    await page.goto(this.url)
    await this.backend.waitReady(60_000)
  }
}
