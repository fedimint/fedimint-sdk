import { AndroidTarget } from './android'
import { Target, TargetName } from './types'

const TARGETS: Record<TargetName, () => Target> = {
  android: () => new AndroidTarget(),
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
