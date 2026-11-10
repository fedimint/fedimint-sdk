/* eslint-disable no-console */
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
    throw new Error(`${name} is not set; scripts/e2e/ios.sh exports it`)
  }
  return value
}

/** The SDK's log filter for the run, read from the launch environment
 * (rust/fedimint-sdk/src/apple/oslog.rs): its default, plus the resolver's
 * trace line listing the DNS servers it read. */
const SDK_LOG_FILTER =
  'warn,fm=info,fedimint=info,n0_dns_resolver::system_config=trace'

/** Real-device only: what WebDriverAgent is signed with, from the
 * environment so no team is written into the repo. */
function signingCapabilities(): Record<string, string | boolean> {
  if (process.env.IOS_REAL_DEVICE !== '1') return {}
  return {
    'appium:xcodeOrgId': requireEnv('IOS_TEAM_ID'),
    'appium:xcodeSigningId': 'Apple Development',
    // A free team needs a bundle id of its own for WebDriverAgent.
    'appium:updatedWDABundleId':
      process.env.IOS_WDA_BUNDLE_ID || `${requireEnv('BUNDLE_ID')}.wda`,
    'appium:allowProvisioningDeviceRegistration': true,
  }
}

/**
 * The iOS harness app (ios/Harness, `org.fedimint.harness`) on a simulator,
 * or on a connected device when scripts/e2e/ios.sh was given IOS_DEVICE,
 * driven through XCUITest. The script has already booted the simulator or
 * found the device, installed the app and, on a simulator, started the SDK's
 * log capture. The simulator shares the host's network, so nothing is
 * forwarded; a device cannot reach the host's loopback at all, which is
 * where devimint lives, so on a device only the federation-free tests run.
 *
 * Typing through XCUITest can drop or alter characters, so the client reads
 * every request back before running it (`verifyInput`).
 */
export class IosTarget implements Target {
  readonly name = 'ios'
  readonly knownUnsupported = {}

  private readonly bundleId = process.env.BUNDLE_ID || 'org.fedimint.harness'
  private readonly deviceId = requireEnv('DEVICE_ID')
  private readonly appPath = requireEnv('APP_PATH')
  private driver: WebdriverIO.Browser | null = null
  private backend: AppiumConsoleBackend | null = null

  async start(): Promise<ConsoleBackend> {
    const appiumHome = process.env.APPIUM_HOME || `${process.cwd()}/.appium`
    this.driver = await remote({
      protocol: 'http',
      hostname: '127.0.0.1',
      port: parseInt(process.env.APPIUM_PORT || '4723', 10),
      path: '/',
      logLevel: process.env.DEBUG_MODE ? 'info' : 'warn',
      // A first session builds WebDriverAgent, which takes minutes; a retry
      // on the default 2-minute timeout would only start a second build.
      connectionRetryTimeout: 600_000,
      connectionRetryCount: 0,
      capabilities: {
        platformName: 'iOS',
        'appium:automationName': 'XCUITest',
        'appium:udid': this.deviceId,
        'appium:bundleId': this.bundleId,
        // The script installs the app; `reset()` below wipes and launches it.
        'appium:noReset': true,
        'appium:autoLaunch': false,
        // WebDriverAgent is built once into APPIUM_HOME and reused.
        'appium:derivedDataPath': `${appiumHome}/wda`,
        'appium:wdaLaunchTimeout': 300000,
        'appium:wdaConnectionTimeout': 300000,
        'appium:simulatorStartupTimeout': 300000,
        'appium:newCommandTimeout': 600,
        'appium:showXcodeLog': !!process.env.DEBUG_MODE,
        ...signingCapabilities(),
      },
    })
    this.backend = new AppiumConsoleBackend(this.driver, (id) => `~${id}`, true)
    // Every run starts from a fresh install, whatever an earlier one left.
    await this.reset()
    return this.backend
  }

  async restart(): Promise<void> {
    await this.session().execute('mobile: terminateApp', {
      bundleId: this.bundleId,
    })
    await this.launch()
  }

  async reset(): Promise<void> {
    const driver = this.session()
    await driver.execute('mobile: terminateApp', { bundleId: this.bundleId })
    try {
      await driver.execute('mobile: clearApp', { bundleId: this.bundleId })
    } catch (error) {
      // Not every driver version clears an app's container; a reinstall
      // does the same.
      console.log(`clearApp failed (${(error as Error).message}); reinstalling`)
      await driver.execute('mobile: removeApp', { bundleId: this.bundleId })
      await driver.execute('mobile: installApp', { app: this.appPath })
    }
    await this.launch()
  }

  async captureFailure(test: string): Promise<void> {
    try {
      await this.backend?.captureUi(
        artifactsDir(),
        `${test}-failure-${Date.now()}`,
      )
    } catch (error) {
      console.error(`Could not capture the screen: ${(error as Error).message}`)
    }
  }

  async afterRun(ran: readonly string[]): Promise<string[]> {
    const logFile = process.env.SDK_LOG_FILE
    if (!logFile) return []
    // The fallback warning fails here too. Whether the Apple resolver logs
    // the servers it read is not established yet, so that line is not
    // required.
    return checkDnsConfig(logFile, currentShape(), ran, false)
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
    if (!this.driver) throw new Error('The iOS session is not started')
    return this.driver
  }

  private async launch(): Promise<void> {
    await this.session().execute('mobile: launchApp', {
      bundleId: this.bundleId,
      environment: { FEDIMINT_SDK_LOG: SDK_LOG_FILTER },
    })
    await this.backend?.waitReady(60_000)
  }
}
