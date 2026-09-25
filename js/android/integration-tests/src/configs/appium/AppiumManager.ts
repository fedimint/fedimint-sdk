/* eslint-disable no-console */
import { remote } from 'webdriverio'

import { AppiumConfigValidator } from './AppiumConfigValidator'
import { AppiumConfig } from './types'

// Single-actor only — this SDK has no flow that needs two simultaneous
// devices. If one ever does, extend this with a per-actor handle scheme
// (device id/AVD suffixes and indexed capabilities like systemPort per handle).
const getCapabilities = (): AppiumConfig => {
  const config = AppiumConfigValidator.getValidatedConfig()

  const udid = process.env.DEVICE_ID
  const avd = process.env.AVD
  if (!udid && !avd) {
    throw new Error('Need DEVICE_ID or AVD set to start an Appium session')
  }

  return {
    'appium:platformName': 'Android',
    'appium:platformVersion': config.PLATFORM_VERSION || '',
    'appium:app': config.BUNDLE_PATH || process.env.BUNDLE_PATH || '',
    'appium:avd': avd,
    'appium:udid': udid,
    'appium:automationName': 'UiAutomator2',
    'appium:appPackage': config.APP_PACKAGE || process.env.APP_PACKAGE || '',
    'appium:appActivity': config.APP_ACTIVITY || process.env.APP_ACTIVITY || '',
    // Auto-grant all runtime perms at install so no permission dialog
    // blocks the run.
    'appium:autoGrantPermissions': true,
    'appium:uiautomator2ServerInstallTimeout': 120000,
    'appium:uiautomator2ServerLaunchTimeout': 120000,
    'appium:newCommandTimeout': 600,
    'appium:systemPort': 8200,
    'appium:chromedriverAutodownload': true,
  }
}

export default class AppiumManager {
  private static driver: WebdriverIO.Browser | null = null
  private static envValidated = false

  static async setupSession(): Promise<WebdriverIO.Browser> {
    if (AppiumManager.driver) {
      console.log('Reusing existing Appium session')
      return AppiumManager.driver
    }

    if (!AppiumManager.envValidated) {
      try {
        AppiumConfigValidator.validateEnvironment()
      } catch (error) {
        console.error(
          '❌ Configuration validation failed:',
          (error as Error).message,
        )
        throw error
      }
      AppiumManager.envValidated = true
    }

    const appiumPort = parseInt(process.env.APPIUM_PORT || '4723', 10)
    const debugMode =
      process.env.DEBUG_MODE === '1' || process.env.DEBUG_MODE === 'true'

    console.log('Initializing Appium session...')
    const driver = await remote({
      protocol: 'http',
      hostname: '127.0.0.1',
      port: appiumPort,
      path: '/',
      logLevel: debugMode ? 'info' : 'warn',
      capabilities: getCapabilities(),
    })
    AppiumManager.driver = driver

    // An idle device's screen blanks at the ~30s default, after which
    // the UI tree empties and findElement returns nothing.
    try {
      await driver.executeScript('mobile: shell', [
        {
          command: 'settings',
          args: ['put', 'system', 'screen_off_timeout', '2147483647'],
        },
      ])
      await driver.executeScript('mobile: shell', [
        { command: 'svc', args: ['power', 'stayon', 'true'] },
      ])
    } catch (error) {
      console.warn(`Could not disable screen-off: ${(error as Error).message}`)
    }

    console.log('Appium session initialized')
    return driver
  }

  static async teardownSession(): Promise<void> {
    if (!AppiumManager.driver) return
    console.log('Terminating Appium session...')
    try {
      await AppiumManager.driver.deleteSession()
    } catch (error) {
      console.error(`Error terminating session: ${(error as Error).message}`)
    }
    AppiumManager.driver = null
    console.log('Appium session terminated')
  }

  static getDriver(): WebdriverIO.Browser | null {
    return AppiumManager.driver
  }

  static deviceId(): string | undefined {
    return process.env.DEVICE_ID
  }
}
