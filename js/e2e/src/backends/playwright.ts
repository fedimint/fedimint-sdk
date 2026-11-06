import fs from 'fs'
import path from 'path'

import type { Page } from 'playwright'

import { parseResponse } from '../protocol/envelope'

import { ConsoleBackend } from './types'

/**
 * The console in a browser page. `page` is read on every use, because a
 * restart replaces the page (and a reset the whole context).
 */
export class PlaywrightConsoleBackend implements ConsoleBackend {
  readonly verifyInput = false

  constructor(private readonly page: () => Page) {}

  async waitReady(timeoutMs: number): Promise<void> {
    const deadline = Date.now() + timeoutMs
    let last = ''
    while (Date.now() < deadline) {
      last = await this.readResponse().catch(() => '')
      if (parseResponse(last)) return
      await new Promise((r) => setTimeout(r, 250))
    }
    throw new Error(
      `The harness page did not come up within ${timeoutMs}ms` +
        (last ? ` — response view: ${last.slice(0, 200)}` : ''),
    )
  }

  async writeRequest(json: string): Promise<void> {
    await this.page().fill('#request', json)
  }

  async readRequest(): Promise<string> {
    return this.page().inputValue('#request')
  }

  async run(): Promise<void> {
    await this.page().click('#run')
  }

  async readResponse(): Promise<string> {
    return (await this.page().textContent('#response')) ?? ''
  }

  async captureUi(dir: string, label: string): Promise<void> {
    fs.mkdirSync(dir, { recursive: true })
    await this.page().screenshot({ path: path.join(dir, `${label}.png`) })
    fs.writeFileSync(
      path.join(dir, `${label}.html`),
      await this.page().content(),
    )
  }
}
