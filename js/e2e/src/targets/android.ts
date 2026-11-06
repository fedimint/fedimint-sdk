/* eslint-disable no-console */
import { execFileSync } from 'child_process'
import fs from 'fs'
import path from 'path'

import { remote } from 'webdriverio'

import { AppiumConsoleBackend } from '../backends/appium'
import { ConsoleBackend } from '../backends/types'
import { artifactsDir } from '../env/constants'
import { currentShape } from '../env/shape'

import { checkDnsConfig } from './logs'
import { Target } from './types'

function requireEnv(name: string): string {
  const value = process.env[name]
  if (!value) {
    throw new Error(`${name} is not set; scripts/e2e/android.sh exports it`)
  }
  return value
}

/**
 * The debug APK's console harness (`harness.ConsoleActivity`), driven through
 * UiAutomator2. scripts/e2e/android.sh has already installed the APK, wiped
 * its data, forwarded devimint's ports and started logcat.
 *
 * The activity has no launcher entry, so every launch names it explicitly
 * rather than going through `activateApp`.
 */
export class AndroidTarget implements Target {
  readonly name = 'android'
  readonly knownUnsupported = {}

  private readonly appPackage = requireEnv('APP_PACKAGE')
  private readonly deviceId = requireEnv('DEVICE_ID')
  private readonly activity = `${this.appPackage}/.harness.ConsoleActivity`
  private driver: WebdriverIO.Browser | null = null
  private backend: AppiumConsoleBackend | null = null

  async start(): Promise<ConsoleBackend> {
    this.driver = await remote({
      protocol: 'http',
      hostname: '127.0.0.1',
      port: parseInt(process.env.APPIUM_PORT || '4723', 10),
      path: '/',
      logLevel: process.env.DEBUG_MODE ? 'info' : 'warn',
      capabilities: {
        platformName: 'Android',
        'appium:automationName': 'UiAutomator2',
        'appium:udid': this.deviceId,
        'appium:appPackage': this.appPackage,
        'appium:appActivity': '.harness.ConsoleActivity',
        // The script installs the app; `reset()` below wipes and launches it.
        'appium:noReset': true,
        'appium:autoLaunch': false,
        'appium:autoGrantPermissions': true,
        'appium:uiautomator2ServerInstallTimeout': 120000,
        'appium:uiautomator2ServerLaunchTimeout': 120000,
        'appium:newCommandTimeout': 600,
        'appium:systemPort': 8200,
      },
    })
    // An idle device blanks its screen after ~30s, and the UI tree with it.
    for (const args of [
      ['settings', ['put', 'system', 'screen_off_timeout', '2147483647']],
      ['svc', ['power', 'stayon', 'true']],
    ] as const) {
      try {
        await this.driver.execute('mobile: shell', {
          command: args[0],
          args: [...args[1]],
        })
      } catch (error) {
        console.warn(
          `Could not keep the screen on: ${(error as Error).message}`,
        )
      }
    }
    this.backend = new AppiumConsoleBackend(
      this.driver,
      (id) =>
        `android=new UiSelector().resourceId("${this.appPackage}:id/${id}")`,
      false,
    )
    // Every run starts from a fresh install, whatever an earlier one left.
    await this.reset()
    return this.backend
  }

  async restart(): Promise<void> {
    await this.session().execute('mobile: terminateApp', {
      appId: this.appPackage,
    })
    await this.launch()
  }

  async reset(): Promise<void> {
    const driver = this.session()
    await driver.execute('mobile: terminateApp', { appId: this.appPackage })
    await driver.execute('mobile: clearApp', { appId: this.appPackage })
    await this.launch()
  }

  async captureFailure(test: string): Promise<void> {
    const dir = artifactsDir()
    const label = `${test}-failure-${Date.now()}`
    try {
      await this.backend?.captureUi(dir, label)
    } catch (error) {
      console.error(`Could not capture the screen: ${(error as Error).message}`)
    }
    try {
      const logcat = execFileSync(
        'adb',
        ['-s', this.deviceId, 'logcat', '-d'],
        {
          maxBuffer: 64 * 1024 * 1024,
        },
      )
      fs.writeFileSync(path.join(dir, `${label}-logcat.log`), logcat)
    } catch (error) {
      console.error(`Could not capture logcat: ${(error as Error).message}`)
    }
  }

  async afterRun(ran: readonly string[]): Promise<string[]> {
    const logFile = process.env.SDK_LOG_FILE
    if (!logFile)
      return ['SDK_LOG_FILE is not set, so the DNS check could not run']
    return checkDnsConfig(logFile, currentShape(), ran, true)
  }

  async stop(): Promise<void> {
    if (!this.driver) return
    try {
      await this.driver.deleteSession()
    } catch (error) {
      console.error(`Error ending the session: ${(error as Error).message}`)
    }
    this.driver = null
  }

  private session(): WebdriverIO.Browser {
    if (!this.driver) throw new Error('The Android session is not started')
    return this.driver
  }

  private async launch(): Promise<void> {
    await this.session().execute('mobile: startActivity', {
      intent: this.activity,
      wait: true,
    })
    await this.backend?.waitReady(60_000)
  }
}
