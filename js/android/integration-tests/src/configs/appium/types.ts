// Android-only for now. Kept as an enum (rather than a hardcoded string) so
// adding iOS later is additive here and in AppiumManager, not a rewrite.
export enum Platform {
  ANDROID = 'android',
}

export const currentPlatform: Platform = Platform.ANDROID

export interface AppiumConfig {
  'appium:platformName': string
  'appium:automationName': string
  'appium:udid'?: string
  'appium:appPackage'?: string
  'appium:app'?: string
  'appium:appActivity'?: string
  'appium:avd'?: string
  'appium:platformVersion'?: string
  [key: `appium:${string}`]: string | number | boolean | undefined
}

export interface LocatorStrategy {
  selector: string
  priority: number
  description?: string
}

export interface ScrollCoordinates {
  startX: number
  startY: number
  endX: number
  endY: number
}

type CreateUnion<
  Max extends number,
  Accumulator extends number[] = [],
> = Accumulator['length'] extends Max
  ? Accumulator[number]
  : CreateUnion<Max, [...Accumulator, Accumulator['length']]>
type IntRange<Min extends number, Max extends number> = Exclude<
  CreateUnion<Max>,
  CreateUnion<Min>
>
export type Percentage = IntRange<1, 101>

export type ScrollDirection = 'up' | 'down' | 'left' | 'right'

export interface ScrollOptions {
  maxScrolls?: number
  scrollDirection?: ScrollDirection
  scrollDuration?: number
  scrollPercentage?: Percentage
}

export interface EnvVarValidation {
  name: string
  validator?: (value: string) => boolean
  errorMessage?: string
}
