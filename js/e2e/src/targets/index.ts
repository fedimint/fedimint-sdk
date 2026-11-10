import { AndroidTarget } from './android'
import { IosTarget } from './ios'
import { WebTarget } from './web'
import { Target, TargetName } from './types'

const TARGETS: Record<TargetName, () => Target> = {
  android: () => new AndroidTarget(),
  ios: () => new IosTarget(),
  web: () => new WebTarget(),
}

/** The target `E2E_TARGET` names. */
export function createTarget(name = process.env.E2E_TARGET): Target {
  const make = TARGETS[name as TargetName]
  if (!make) {
    throw new Error(
      `E2E_TARGET is "${name ?? ''}" — expected one of ${Object.keys(TARGETS).join(', ')}`,
    )
  }
  return make()
}
