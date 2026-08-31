/* eslint-disable no-console */
import { execFileSync } from 'child_process'
import fs from 'fs'
import path from 'path'

import AppiumManager from './configs/appium/AppiumManager'
import { AppiumTestBase } from './configs/appium/AppiumTestBase'
import { Fixture } from './fixtures/types'
import { availableTests, resolveTestNames, TestName } from './registry'

// No fixtures registered yet — the v1 MnemonicService test needs none. Add
// entries here as `{ [fixture.produces]: fixture }` when a test declares a
// `static prerequisites` state (e.g. "joinedFederation") that isn't the
// fresh-install default.
const fixtures: Record<string, Fixture> = {}

function captureAndroidLogcat(testName: string) {
  const outDir = path.join(process.cwd(), '.appium')
  if (!fs.existsSync(outDir)) fs.mkdirSync(outDir, { recursive: true })
  const udid = AppiumManager.deviceId()
  if (!udid) return
  const outPath = path.join(outDir, `${testName}-failure-logcat.log`)
  try {
    const out = execFileSync('adb', ['-s', udid, 'logcat', '-d'], {
      maxBuffer: 64 * 1024 * 1024,
    })
    fs.writeFileSync(outPath, out)
    console.log(`Logcat saved to: ${outPath}`)
  } catch (logError) {
    console.error('Failed to capture logcat:', logError)
  }
}

// Mutated by ensureState as fixtures run; cleared on test failure (state
// untrusted after a failure/reset).
const currentState = new Set<string>()

function resolvePlan(
  needed: readonly string[],
  have: ReadonlySet<string>,
): Fixture[] {
  const plan: Fixture[] = []
  const visiting = new Set<string>()
  const planned = new Set<string>()

  function visit(state: string): void {
    if (have.has(state) || planned.has(state)) return
    if (visiting.has(state)) {
      throw new Error(`Cyclic fixture dependency at "${state}"`)
    }
    const fixture = fixtures[state]
    if (!fixture) {
      throw new Error(
        `No fixture produces state "${state}" — add one to fixtures or remove the prerequisite`,
      )
    }
    visiting.add(state)
    for (const req of fixture.requires) visit(req)
    visiting.delete(state)
    planned.add(state)
    plan.push(fixture)
  }

  for (const state of needed) visit(state)
  return plan
}

async function ensureState(
  test: AppiumTestBase,
  needed: readonly string[],
): Promise<void> {
  const neededSet = new Set(needed)
  const extra = [...currentState].filter((t) => !neededSet.has(t))

  if (extra.length > 0) {
    console.log(
      `State has [${extra.join(', ')}] beyond what test requires — resetting`,
    )
    await test.resetAppToFresh()
    currentState.clear()
  }

  const plan = resolvePlan(needed, currentState)
  for (const fixture of plan) {
    await fixture.run(test)
    currentState.add(fixture.produces)
  }
}

let anyTestFailed = false

async function waitForMetroBundleComplete(): Promise<void> {
  const timeout = 180000
  const startTime = Date.now()
  console.log('Checking if Metro bundle is complete...')

  return new Promise((resolve, reject) => {
    const checkInterval = setInterval(async () => {
      try {
        const response = await fetch('http://localhost:8081/status')
        const status = await response.text()

        if (status.includes('packager-status:running')) {
          const packagerResponse = await fetch(
            'http://localhost:8081/index.bundle?platform=android&dev=true&minify=false&status=true',
          )
          if (packagerResponse.status === 200) {
            console.log('Bundle is ready!')
            clearInterval(checkInterval)
            resolve()
            return
          }
        }
        if (Date.now() - startTime > timeout) {
          clearInterval(checkInterval)
          reject(new Error('Timed out waiting for Metro bundle'))
        }
      } catch {
        if (Date.now() - startTime > timeout) {
          clearInterval(checkInterval)
          reject(new Error('Timed out waiting for Metro bundle'))
        }
      }
    }, 5000)
  })
}

async function runTests(testNames: string[]): Promise<void> {
  try {
    const validTestNames = testNames.filter((name) =>
      Object.keys(availableTests).includes(name),
    ) as TestName[]

    if (validTestNames.length === 0) {
      console.error(
        'No valid tests selected. Available tests:',
        Object.keys(availableTests).join(', '),
      )
      anyTestFailed = true
      return
    }

    try {
      await waitForMetroBundleComplete()
    } catch (error) {
      console.error('Failed to verify Metro bundle:', error)
      anyTestFailed = true
      return
    }

    console.log(`Running the following tests: ${validTestNames.join(', ')}`)

    const results: Record<string, { success: boolean; error?: unknown }> = {}

    for (const testName of validTestNames) {
      console.log(`\n=== Starting test: ${testName} ===`)

      const TestClass = availableTests[testName]
      const test = new TestClass()

      try {
        await test.initialize()

        // Recorded for every test, but only ever saved to disk on failure
        // (below) — same cost model as the screenshot/page-source capture.
        // A passing run pays for start+stop, not for writing/uploading a
        // video nobody needs.
        try {
          await test.driver.startRecordingScreen()
        } catch (recError) {
          console.warn('Could not start screen recording:', recError)
        }

        await ensureState(test, TestClass.prerequisites)
        await test.execute()

        try {
          await test.driver.stopRecordingScreen()
        } catch {
          // Nothing to clean up if it never started.
        }

        for (const state of TestClass.produces) currentState.add(state)

        results[testName] = { success: true }
        console.log(`=== Test ${testName} completed successfully ===\n`)
      } catch (error: unknown) {
        results[testName] = { success: false, error }
        console.error(
          `=== Test ${testName} failed: ${(error as Error).message} ===\n`,
        )
        anyTestFailed = true

        const drv = AppiumManager.getDriver()
        if (drv) {
          try {
            const screenshot = await drv.takeScreenshot()
            const screenshotPath = path.join(
              process.cwd(),
              'screenshots',
              `${testName}-failure-${Date.now()}.png`,
            )
            const dir = path.dirname(screenshotPath)
            if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true })
            fs.writeFileSync(screenshotPath, screenshot, 'base64')
            console.log(`Screenshot saved to: ${screenshotPath}`)

            const pageSource = await drv.getPageSource()
            const xmlPath = path.join(
              process.cwd(),
              'screenshots',
              `${testName}-failure-${Date.now()}.xml`,
            )
            fs.writeFileSync(xmlPath, pageSource)
            console.log(`Page source saved to: ${xmlPath}`)
          } catch (captureError) {
            console.error(
              'Failed to capture screenshot/page source:',
              captureError,
            )
          }

          try {
            const video = await drv.stopRecordingScreen()
            if (video) {
              const videoPath = path.join(
                process.cwd(),
                'screenshots',
                `${testName}-failure-${Date.now()}.mp4`,
              )
              fs.writeFileSync(videoPath, video, 'base64')
              console.log(`Recording saved to: ${videoPath}`)
            }
          } catch (recordingError) {
            console.error('Failed to save screen recording:', recordingError)
          }
        }

        captureAndroidLogcat(testName)

        try {
          await test.resetAppToFresh()
        } catch (resetError) {
          console.error(
            'Reset after failure failed. Subsequent tests may not run cleanly:',
            resetError,
          )
        }
        currentState.clear()
      }
    }

    console.log('\n=== Test Run Summary ===')
    for (const [testName, result] of Object.entries(results)) {
      console.log(`${testName}: ${result.success ? 'PASSED' : 'FAILED'}`)
      if (!result.success) {
        console.log(`  Error: ${(result.error as Error).message}`)
      }
    }

    const successCount = Object.values(results).filter((r) => r.success).length
    console.log(`\n${successCount}/${validTestNames.length} tests passed.`)
    if (successCount < validTestNames.length) anyTestFailed = true
  } catch (error) {
    console.error('Test run failed:', error)
    anyTestFailed = true
  } finally {
    try {
      await AppiumManager.teardownSession()
    } catch (error) {
      console.error('Error during teardown:', error)
    }

    if (anyTestFailed) {
      console.error('\n❌ Tests failed')
      process.exit(1)
    } else {
      console.log('\n✅ All tests passed')
      process.exit(0)
    }
  }
}

const testNames = process.argv.slice(2)

if (testNames.length === 0) {
  console.log(
    'No tests specified. Available tests:',
    Object.keys(availableTests).join(', '),
  )
  console.log('Usage: ts-node src/runner.ts [test1] [test2] ...')
  process.exit(1)
} else {
  runTests(resolveTestNames(testNames))
}

process.on('SIGINT', async () => {
  console.log('\nReceived SIGINT. Shutting down gracefully...')
  try {
    await AppiumManager.teardownSession()
  } catch (error) {
    console.error('Error during teardown after SIGINT:', error)
  }
  process.exit(2)
})
