import fs from 'fs'
import path from 'path'

import { parseResponse } from '../protocol/envelope'

import { ConsoleBackend } from './types'

type ElementId = 'request' | 'run' | 'response'

/**
 * The console over an Appium session. `selector` turns an element id into the
 * platform's locator (a resource-id on Android, an accessibility id on iOS).
 * Elements are looked up on every use rather than cached, so a relaunched app
 * never leaves a stale reference behind.
 */
export class AppiumConsoleBackend implements ConsoleBackend {
  constructor(
    private readonly driver: WebdriverIO.Browser,
    private readonly selector: (id: ElementId) => string,
    readonly verifyInput: boolean,
  ) {}

  private element(id: ElementId) {
    return this.driver.$(this.selector(id))
  }

  async waitReady(timeoutMs: number): Promise<void> {
    const deadline = Date.now() + timeoutMs
    let last = ''
    while (Date.now() < deadline) {
      try {
        last = await this.readResponse()
        if (parseResponse(last)) return
      } catch {
        // Not on screen yet.
      }
      await new Promise((r) => setTimeout(r, 500))
    }
    throw new Error(
      `The harness console did not come up within ${timeoutMs}ms` +
        (last ? ` — response view: ${last.slice(0, 200)}` : ''),
    )
  }

  async writeRequest(json: string): Promise<void> {
    await this.element('request').setValue(json)
  }

  async readRequest(): Promise<string> {
    return this.element('request').getText()
  }

  async run(): Promise<void> {
    await this.element('run').click()
  }

  async readResponse(): Promise<string> {
    return this.element('response').getText()
  }

  async captureUi(dir: string, label: string): Promise<void> {
    fs.mkdirSync(dir, { recursive: true })
    const png = await this.driver.takeScreenshot()
    fs.writeFileSync(path.join(dir, `${label}.png`), png, 'base64')
    fs.writeFileSync(
      path.join(dir, `${label}.xml`),
      await this.driver.getPageSource(),
    )
  }
}
