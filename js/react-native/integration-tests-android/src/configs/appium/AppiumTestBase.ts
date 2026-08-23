/* eslint-disable no-console */
import fs from 'fs'
import path from 'path'
import { ChainablePromiseArray, ChainablePromiseElement } from 'webdriverio'

import AppiumManager from './AppiumManager'
import {
  LocatorStrategy,
  Percentage,
  ScrollCoordinates,
  ScrollDirection,
  ScrollOptions,
} from './types'

// Android-only, single-device test base. Deliberately excludes (add back if
// a test needs them): iOS branches, PWA branches, multi-actor (spawnActor),
// deep links, in-app webview context switching, long-press.

const DEFAULT_SCROLL_OPTIONS: Required<ScrollOptions> = {
  maxScrolls: 10,
  scrollDirection: 'down',
  scrollDuration: 100,
  scrollPercentage: 10,
}

export const DEFAULT_TIMEOUT = 20000

// Longer budget for operations that touch a real federation over the
// network (join, invoice pay/create) rather than purely-local SDK calls.
export const NETWORK_TIMEOUT = 60000

const E2E_DEBUG =
  process.env.DEBUG_MODE === '1' || process.env.DEBUG_MODE === 'true'

const debugLog = (...args: unknown[]) => {
  if (E2E_DEBUG) console.log(...args)
}

export class AppiumTestBase {
  /** States required before execute(); default [] = fresh install. */
  static prerequisites: readonly string[] = []

  /** States the device satisfies after a successful execute(). */
  static produces: readonly string[] = []

  driver!: WebdriverIO.Browser

  async initialize(): Promise<void> {
    this.driver = await AppiumManager.setupSession()
  }

  async teardown(): Promise<void> {
    await AppiumManager.teardownSession()
  }

  async execute(): Promise<void> {
    throw new Error(
      `${this.constructor.name}.execute() must be overridden by the test class`,
    )
  }

  getElementLocatorStrategies(key: string): LocatorStrategy[] {
    return [
      {
        selector: `accessibility id:${key}`,
        priority: 1,
        description: 'Accessibility ID',
      },
      {
        selector: `android=new UiSelector().resourceId("${key}")`,
        priority: 2,
        description: 'Resource ID',
      },
    ]
  }

  async findElementByKey(key: string): Promise<ChainablePromiseElement | null> {
    const strategies = this.getElementLocatorStrategies(key).sort(
      (a, b) => a.priority - b.priority,
    )
    const errors: string[] = []

    for (const strategy of strategies) {
      try {
        debugLog(
          `Trying to find element with strategy: ${strategy.description}`,
        )
        const element = await this.driver.$(strategy.selector)
        if (await element.isExisting()) {
          debugLog(`Element found with strategy: ${strategy.description}`)
          return element
        }
        errors.push(`Element not found with strategy: ${strategy.description}`)
      } catch (error: unknown) {
        errors.push(
          `Strategy ${strategy.description} failed: ${(error as Error).message}`,
        )
      }
    }

    if (E2E_DEBUG) {
      debugLog(
        `Element with key "${key}" not found after trying all strategies:`,
      )
      errors.forEach((err, index) => debugLog(`  ${index + 1}. ${err}`))
    }
    return null
  }

  getTextLocatorStrategies(
    text: string,
    exactMatch: boolean,
  ): LocatorStrategy[] {
    if (exactMatch) {
      return [
        {
          selector: `android=new UiSelector().text("${text}")`,
          priority: 1,
          description: 'Exact text match',
        },
        {
          selector: `android=new UiSelector().description("${text}")`,
          priority: 2,
          description: 'Exact content description',
        },
      ]
    }
    return [
      {
        selector: `android=new UiSelector().textContains("${text}")`,
        priority: 1,
        description: 'Partial text match',
      },
      {
        selector: `android=new UiSelector().descriptionContains("${text}")`,
        priority: 2,
        description: 'Partial content description',
      },
    ]
  }

  async findElementsByText(
    text: string,
    exactMatch = false,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<ChainablePromiseArray> {
    const startTime = Date.now()
    const strategies = this.getTextLocatorStrategies(text, exactMatch).sort(
      (a, b) => a.priority - b.priority,
    )

    while (Date.now() - startTime < timeout) {
      for (const strategy of strategies) {
        try {
          const elements = await this.driver
            .$$(strategy.selector)
            .filter((el) => el.isDisplayed())
          if (elements.length > 0) {
            return this.driver.$$(elements)
          }
        } catch (error) {
          debugLog(
            `No elements found using ${strategy.description}. ${(error as Error).message}.`,
          )
        }
        await new Promise((resolve) => setTimeout(resolve, 500))
      }
    }
    console.log(
      `No elements with "${text}" in them were found. If this is not intentional, check the page source dump.`,
    )
    return this.driver.$$([])
  }

  async findElementByText(
    text: string,
    instanceNum: number,
    exactMatch = false,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<ChainablePromiseElement | null> {
    const elements = await this.findElementsByText(text, exactMatch, timeout)
    const count = await elements.length
    if (count === 0) return null

    if (instanceNum < 0) {
      const actualIndex = count + instanceNum
      return actualIndex >= 0 ? elements[actualIndex] : null
    }
    return instanceNum < count ? elements[instanceNum] : null
  }

  async isTextPresent(
    text: string,
    exactMatch = false,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<boolean> {
    const elements = await this.findElementsByText(text, exactMatch, timeout)
    return (await elements.length) > 0
  }

  async waitForText(
    text: string,
    instanceNum: number,
    exactMatch = false,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<ChainablePromiseElement> {
    const element = await this.findElementByText(
      text,
      instanceNum,
      exactMatch,
      timeout,
    )
    if (!element) {
      throw new Error(
        `Text "${text}" (instance ${instanceNum}) not found on screen within ${timeout}ms`,
      )
    }
    return element
  }

  async clickOnText(
    text: string,
    instanceNum: number,
    exactMatch = false,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<void> {
    const element = await this.waitForText(
      text,
      instanceNum,
      exactMatch,
      timeout,
    )
    await element.click()
  }

  private async isElementVisible(
    element: ChainablePromiseElement | null,
  ): Promise<boolean> {
    return element !== null && (await element.isDisplayed())
  }

  async waitForElementDisplayed(
    key: string,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<ChainablePromiseElement> {
    const startTime = Date.now()
    const errors: Error[] = []

    while (Date.now() - startTime < timeout) {
      try {
        const element = await this.findElementByKey(key)
        if (await this.isElementVisible(element)) {
          return element as ChainablePromiseElement
        }
      } catch (error) {
        errors.push(error as Error)
      }
      await new Promise((resolve) => setTimeout(resolve, 500))
    }
    throw new Error(
      `Element with key "${key}" not displayed after ${timeout}ms. Errors: ${errors.map((e) => e.message).join('; ')}`,
    )
  }

  // Polls until the element is no longer displayed — the inverse of
  // waitForElementDisplayed, for "this went away" assertions.
  async waitForElementGone(
    key: string,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<void> {
    const startTime = Date.now()
    while (Date.now() - startTime < timeout) {
      const element = await this.findElementByKey(key)
      const visible =
        element !== null && (await element.isDisplayed().catch(() => false))
      if (!visible) return
      await new Promise((resolve) => setTimeout(resolve, 500))
    }
    throw new Error(
      `Element with key "${key}" still displayed after ${timeout}ms`,
    )
  }

  // Writes a screenshot to the same screenshots/ dir the runner uploads on
  // failure, so success-path UI can be eyeballed in the run artifacts too.
  async saveScreenshot(label: string): Promise<void> {
    try {
      const png = await this.driver.takeScreenshot()
      const dir = path.join(process.cwd(), 'screenshots')
      if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true })
      const file = path.join(dir, `${label}.png`)
      fs.writeFileSync(file, png, 'base64')
      console.log(`Screenshot saved to: ${file}`)
    } catch (error) {
      console.error(`saveScreenshot("${label}") failed:`, error)
    }
  }

  private async isElementClickable(
    element: ChainablePromiseElement,
  ): Promise<boolean> {
    const attr = await element.getAttribute('clickable')
    return attr === 'true'
  }

  async clickElementByKey(
    key: string,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<void> {
    console.log(`Attempting to click element: ${key}`)
    const element = await this.waitForElementDisplayed(key, timeout)

    const startTime = Date.now()
    while (Date.now() - startTime < timeout) {
      if (await this.isElementClickable(element)) {
        await element.click()
        console.log(`Successfully clicked element: ${key}`)
        return
      }
      await new Promise((resolve) => setTimeout(resolve, 100))
    }
    throw new Error(
      `Element "${key}" was displayed but not clickable after ${timeout}ms`,
    )
  }

  async typeIntoElementByKey(
    key: string,
    text: string,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<void> {
    console.log(`Attempting to type into element: ${key}`)
    const element = await this.waitForElementDisplayed(key, timeout)
    await element.setValue(text)
    console.log(`Successfully typed into element: ${key}`)
  }

  async elementIsDisplayed(
    key: string,
    timeout = DEFAULT_TIMEOUT,
  ): Promise<boolean> {
    try {
      await this.waitForElementDisplayed(key, timeout)
      return true
    } catch (error: unknown) {
      console.log(
        `Element ${key} is not displayed: ${(error as Error).message}`,
      )
      return false
    }
  }

  private async getScrollCoordinates(
    scrollDirection: ScrollDirection,
    scrollPercentage: Percentage,
  ): Promise<ScrollCoordinates> {
    const { width, height } = await this.driver.getWindowSize()
    const margin = (1 - scrollPercentage / 100) / 2

    const coordinatesMap: Record<ScrollDirection, ScrollCoordinates> = {
      down: {
        startX: width / 2,
        startY: height * (1 - margin),
        endX: width / 2,
        endY: height * margin,
      },
      up: {
        startX: width / 2,
        startY: height * margin,
        endX: width / 2,
        endY: height * (1 - margin),
      },
      left: {
        startX: width * margin,
        startY: height / 2,
        endX: width * (1 - margin),
        endY: height / 2,
      },
      right: {
        startX: width * (1 - margin),
        startY: height / 2,
        endX: width * margin,
        endY: height / 2,
      },
    }
    return coordinatesMap[scrollDirection]
  }

  private createScrollActions(
    coordinates: ScrollCoordinates,
    duration: number,
  ) {
    const { startX, startY, endX, endY } = coordinates
    return [
      {
        type: 'pointer',
        id: 'finger1',
        parameters: { pointerType: 'touch' },
        actions: [
          {
            type: 'pointerMove',
            duration: 0,
            x: Math.round(startX),
            y: Math.round(startY),
          },
          { type: 'pointerDown', button: 0 },
          {
            type: 'pointerMove',
            duration,
            x: Math.round(endX),
            y: Math.round(endY),
          },
          { type: 'pointerUp', button: 0 },
        ],
      },
    ]
  }

  async scroll(
    scrollDirection: ScrollDirection,
    scrollDuration = 1000,
    scrollPercentage: Percentage,
  ): Promise<void> {
    try {
      const coordinates = await this.getScrollCoordinates(
        scrollDirection,
        scrollPercentage,
      )
      const actions = this.createScrollActions(coordinates, scrollDuration)
      await this.driver.performActions(actions)
      await this.driver.releaseActions()
      await this.driver.pause(scrollDuration * 2)
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Unknown error'
      throw new Error(`Scroll action failed: ${message}`)
    }
  }

  private async scrollUntilFound(
    findElementFn: () => Promise<ChainablePromiseElement | null>,
    elementDescription: string,
    options: ScrollOptions = {},
  ): Promise<ChainablePromiseElement | null> {
    const config = { ...DEFAULT_SCROLL_OPTIONS, ...options }
    const { maxScrolls, scrollDirection, scrollDuration, scrollPercentage } =
      config

    let element = await findElementFn()
    if (await this.isElementVisible(element)) {
      console.log(`Element ${elementDescription} is already visible`)
      return element
    }

    for (let i = 0; i < maxScrolls; i++) {
      try {
        await this.scroll(scrollDirection, scrollDuration, scrollPercentage)
        element = await findElementFn()
        if (await this.isElementVisible(element)) {
          console.log(`Element found and visible after ${i + 1} scroll(s)`)
          return element
        }
      } catch (error) {
        console.error(`Scroll attempt ${i + 1} failed:`, error)
      }
    }
    console.log(
      `Element ${elementDescription} not found after ${maxScrolls} scroll attempts`,
    )
    return null
  }

  async scrollToElement(
    key: string,
    scrollOptions: ScrollOptions = {},
  ): Promise<ChainablePromiseElement | null> {
    return this.scrollUntilFound(
      () => this.findElementByKey(key),
      `with key "${key}"`,
      scrollOptions,
    )
  }

  async scrollToText(
    text: string,
    instanceNum = 0,
    exactMatch = false,
    timeout?: number,
    scrollOptions: ScrollOptions = {},
  ): Promise<ChainablePromiseElement | null> {
    return this.scrollUntilFound(
      () => this.findElementByText(text, instanceNum, exactMatch, timeout),
      `with text "${text}"`,
      scrollOptions,
    )
  }

  async dismissKeyboard(): Promise<void> {
    try {
      await this.driver.executeScript('mobile: isKeyboardShown', [])
      await this.driver.executeScript('mobile: hideKeyboard', [
        { keys: ['done'] },
      ])
    } catch (error: unknown) {
      console.log(
        `Unable to hide keyboard. Reason: ${(error as Error).message}`,
      )
    }
  }

  async acceptAlert(button: string): Promise<void> {
    try {
      await this.driver.executeScript('mobile: acceptAlert', [
        { buttonLabel: button },
      ])
    } catch (error: unknown) {
      console.log(
        `Unable to accept alert with buttonLabel ${button}. Reason: ${(error as Error).message}. Trying to infer the button.`,
      )
      try {
        await this.driver.executeScript('mobile: acceptAlert', [])
      } catch (fallbackError: unknown) {
        console.log(
          `No system alert detected. Falling back to tapping button by text. Reason: ${(fallbackError as Error).message}`,
        )
        await this.clickOnText(button.toUpperCase(), 0, true)
      }
    }
  }

  async dismissAlert(button: string): Promise<void> {
    try {
      await this.driver.executeScript('mobile: dismissAlert', [
        { buttonLabel: button },
      ])
    } catch (error: unknown) {
      console.log(
        `Unable to dismiss alert with buttonLabel ${button}. Reason: ${(error as Error).message}. Trying to infer the button.`,
      )
      try {
        await this.driver.executeScript('mobile: dismissAlert', [])
      } catch (fallbackError: unknown) {
        console.log(
          `No system alert detected. Falling back to tapping button by text. Reason: ${(fallbackError as Error).message}`,
        )
        await this.clickOnText(button.toUpperCase(), 0, true)
      }
    }
  }

  async getClipboard(): Promise<string> {
    try {
      const base64Content = (await this.driver.executeScript(
        'mobile: getClipboard',
        [],
      )) as string
      if (!base64Content) {
        console.warn('Received empty or invalid clipboard data')
        return ''
      }
      return Buffer.from(base64Content, 'base64').toString('utf8')
    } catch (error: unknown) {
      console.error(`Failed to get clipboard: ${(error as Error).message}`)
      return ''
    }
  }

  async getTextByKey(key: string): Promise<string> {
    const element = await this.findElementByKey(key)
    if (!element) {
      throw new Error(`getTextByKey: element with key "${key}" not found`)
    }
    const text = await element.getText()
    if (text && text.length > 0) return text
    throw new Error(
      `getTextByKey: element with key "${key}" was found but has no readable text`,
    )
  }

  async resetAppToFresh(): Promise<void> {
    console.log('Resetting app to fresh-install state...')
    const appId = process.env.APP_PACKAGE
    if (!appId) {
      throw new Error('resetAppToFresh requires APP_PACKAGE to be set')
    }
    await this.driver.executeScript('mobile: terminateApp', [{ appId }])
    await this.driver.executeScript('mobile: clearApp', [{ appId }])
    await this.driver.executeScript('mobile: activateApp', [{ appId }])
    console.log('App reset complete')
  }
}
