import { usePush } from '../../../../src/navigation/push'
import { FederationsScreen } from '../../../../src/screens/federations/FederationsScreen'

export default function Federations() {
  const push = usePush()
  return (
    <FederationsScreen
      join={() => push('/federations/join')}
      detail={() => push('/federations/federation-detail')}
    />
  )
}
