import { usePush } from '../../../../src/navigation/push'
import { ActivityScreen } from '../../../../src/screens/activity/ActivityScreen'
import { openOperation } from '../../../../src/navigation/options'

export default function Activity() {
  const push = usePush()
  return (
    <ActivityScreen
      showOperation={(item) => openOperation(push, 'activity', item)}
    />
  )
}
