import { usePush } from '../../src/navigation/push'
import { model } from '../../src/model/DemoModel'
import { BackupScreen } from '../../src/screens/onboarding/BackupScreen'

export default function Backup() {
  const push = usePush()
  return (
    <BackupScreen
      next={() => push('/verify')}
      later={() => model.markBackedUp()}
    />
  )
}
