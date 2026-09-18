import { AppiumTestBase } from './configs/appium/AppiumTestBase'
import { InviteCodeService } from './services/InviteCodeService.test'
import { MnemonicService } from './services/MnemonicService.test'

export type TestClass = (new () => AppiumTestBase) & {
  prerequisites: readonly string[]
  produces: readonly string[]
}

export const availableTests: Record<string, TestClass> = {
  mnemonic: MnemonicService,
  inviteCode: InviteCodeService,
}

export type TestName = keyof typeof availableTests

// Resolve CLI test args (which may be "all" or a subset) to concrete names.
export function resolveTestNames(args: string[]): string[] {
  if (args.includes('all')) return Object.keys(availableTests)
  return args
}
