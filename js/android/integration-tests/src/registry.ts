import { AppiumTestBase } from './configs/appium/AppiumTestBase'
import { FederationService } from './services/FederationService.test'
import { InviteCodeService } from './services/InviteCodeService.test'
import { LightningService } from './services/LightningService.test'
import { MintService } from './services/MintService.test'
import { MnemonicService } from './services/MnemonicService.test'

export type TestClass = (new () => AppiumTestBase) & {
  prerequisites: readonly string[]
  produces: readonly string[]
}

// Order matters for a run of `all`: the two local tests come first because
// they need no federation, then the federation-backed ones in an order that
// lets each inherit the last one's state (joined, then funded) instead of
// resetting the app and joining again. The runner enforces the states
// regardless — this ordering only decides how much work it repeats.
export const availableTests: Record<string, TestClass> = {
  mnemonic: MnemonicService,
  inviteCode: InviteCodeService,
  federation: FederationService,
  lightning: LightningService,
  mint: MintService,
}

export type TestName = keyof typeof availableTests

// Resolve CLI test args (which may be "all" or a subset) to concrete names.
export function resolveTestNames(args: string[]): string[] {
  if (args.includes('all')) return Object.keys(availableTests)
  return args
}
