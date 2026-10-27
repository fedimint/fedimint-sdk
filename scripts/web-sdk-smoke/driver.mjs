/**
 * Opens the smoke test's page in headless Chromium and waits for its outcome.
 *
 *   node driver.mjs <playwright dir> <url> <timeout in seconds>
 *
 * <playwright dir> is a directory from which `playwright` resolves: the JavaScript workspace's
 * `js`. The page (scripts/web-sdk-smoke/main.js) leaves its outcome in `window.__fedimintSmoke`.
 * The driver prints the outcome as one line of JSON and exits 0 when its status is `ok`. In every
 * other case, a failed check, an uncaught error or no outcome within the timeout, it prints what
 * it saw (the outcome, the page's console output and its uncaught errors) to stderr and exits 1.
 */
import { createRequire } from 'node:module'
import { join } from 'node:path'

const [dir, url, seconds] = process.argv.slice(2)
if (!dir || !url || !seconds) {
  console.error(
    'usage: node driver.mjs <playwright dir> <url> <timeout in seconds>',
  )
  process.exit(2)
}
const { chromium } = createRequire(join(dir, 'package.json'))('playwright')

const consoleLines = []
const pageErrors = []

const browser = await chromium.launch()
let outcome
let reason
try {
  const page = await browser.newPage()
  page.on('console', (message) =>
    consoleLines.push(`[${message.type()}] ${message.text()}`),
  )
  page.on('pageerror', (error) => pageErrors.push(error.stack ?? String(error)))
  page.on('requestfailed', (request) =>
    consoleLines.push(
      `[requestfailed] ${request.url()}: ${request.failure()?.errorText}`,
    ),
  )
  page.on('response', (response) => {
    if (response.status() >= 400) {
      consoleLines.push(`[http ${response.status()}] ${response.url()}`)
    }
  })

  await page.goto(url)
  try {
    const handle = await page.waitForFunction(
      () => window.__fedimintSmoke,
      null,
      {
        timeout: Number(seconds) * 1000,
        polling: 250,
      },
    )
    outcome = await handle.jsonValue()
  } catch (error) {
    reason = `no outcome within ${seconds} seconds (${error.name})`
  }
} catch (error) {
  reason = `the page could not be opened: ${error.message}`
} finally {
  await browser.close()
}

if (outcome?.status === 'ok' && pageErrors.length === 0) {
  console.log(JSON.stringify(outcome))
  process.exit(0)
}

console.error(
  `the page reported: ${outcome ? JSON.stringify(outcome) : (reason ?? 'nothing')}`,
)
console.error('--- console output')
for (const line of consoleLines) console.error(line)
console.error('--- uncaught errors')
for (const line of pageErrors) console.error(line)
process.exit(1)
