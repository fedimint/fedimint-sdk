import { usePush } from '../../src/navigation/push'
import { WelcomeScreen } from '../../src/screens/onboarding/WelcomeScreen'

export default function Welcome() {
  const push = usePush()
  return <WelcomeScreen restore={() => push('/restore')} />
}
