import { model, useDemo } from '../../src/model/DemoModel'
import { VerifyScreen } from '../../src/screens/onboarding/VerifyScreen'

export default function Verify() {
  const state = useDemo()
  return (
    <VerifyScreen words={state.seedWords} finish={() => model.markBackedUp()} />
  )
}
