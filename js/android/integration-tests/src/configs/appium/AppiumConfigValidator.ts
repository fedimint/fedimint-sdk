/* eslint-disable no-console */
import { EnvVarValidation } from './types'

// Android-only env var validation (a multi-platform version would add
// iOS/PWA branches back in here).
export class AppiumConfigValidator {
  private static optionalVars: string[] = [
    'PLATFORM_VERSION',
    'BUNDLE_PATH',
    'DEVICE_ID',
    'AVD',
  ]

  private static validations: EnvVarValidation[] = [
    {
      name: 'AVD',
      validator: (value) => value.length > 0,
      errorMessage: 'AVD cannot be empty',
    },
    {
      name: 'DEVICE_ID',
      validator: (value) => value.length > 0,
      errorMessage: 'DEVICE_ID cannot be empty',
    },
    {
      name: 'BUNDLE_PATH',
      validator: (value) => value.endsWith('.apk') || value.startsWith('http'),
      errorMessage: 'BUNDLE_PATH must be a valid .apk file path or URL',
    },
    {
      name: 'APP_PACKAGE',
      validator: (value) => value.length > 0,
      errorMessage: 'APP_PACKAGE cannot be empty',
    },
  ]

  static validateEnvironment(): void {
    console.log('🔍 Validating environment variables...')

    if (!process.env.DEVICE_ID && !process.env.AVD) {
      throw new Error(
        'Missing required environment variables: either DEVICE_ID or AVD must be provided\n\n' +
          `Please set one of the following environment variables:\n${this.getExampleConfig()}`,
      )
    }

    if (!process.env.BUNDLE_PATH) {
      throw new Error(
        `Missing required environment variable: BUNDLE_PATH\n\n${this.getExampleConfig()}`,
      )
    }

    if (!process.env.APP_PACKAGE) {
      throw new Error(
        `Missing required environment variable: APP_PACKAGE\n\n${this.getExampleConfig()}`,
      )
    }

    const errors: string[] = []
    for (const validation of this.validations) {
      const value = process.env[validation.name]
      if (value && validation.validator && !validation.validator(value)) {
        errors.push(
          validation.errorMessage || `Invalid value for ${validation.name}`,
        )
      }
    }

    if (errors.length > 0) {
      throw new Error(
        `Environment variable validation failed:\n${errors.join('\n')}`,
      )
    }

    console.log('✅ Environment variables validated successfully')
    this.logConfiguration()
  }

  private static getExampleConfig(): string {
    return `
export AVD=android-34  # Run 'emulator -list-avds' to get this
# OR
export DEVICE_ID=emulator-5554  # Run 'adb devices' to get this
export BUNDLE_PATH=/path/to/app-debug.apk
export APP_PACKAGE=com.reactnativeexample
export APP_ACTIVITY=com.reactnativeexample.MainActivity
export PLATFORM_VERSION=34  # Optional
        `.trim()
  }

  private static logConfiguration(): void {
    console.log('\n📱 Test Configuration:')
    if (process.env.DEVICE_ID)
      console.log(`   Device ID: ${process.env.DEVICE_ID}`)
    if (process.env.AVD) console.log(`   AVD: ${process.env.AVD}`)
    console.log(`   Bundle Path: ${process.env.BUNDLE_PATH}`)
    console.log(`   App Package: ${process.env.APP_PACKAGE}`)
    console.log(`   App Activity: ${process.env.APP_ACTIVITY}`)
    if (process.env.PLATFORM_VERSION) {
      console.log(`   Platform Version: ${process.env.PLATFORM_VERSION}`)
    }
    console.log('')
  }

  static getValidatedConfig(): Record<string, string> {
    const config: Record<string, string> = {}
    for (const varName of [
      ...this.optionalVars,
      'APP_PACKAGE',
      'APP_ACTIVITY',
    ]) {
      if (process.env[varName]) config[varName] = process.env[varName] as string
    }
    return config
  }
}
