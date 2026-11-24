import { useNavigation } from 'expo-router'
import { usePush } from '../../../../src/navigation/push'
import { HomeScreen } from '../../../../src/screens/home/HomeScreen'
import { openOperation } from '../../../../src/navigation/options'

export default function Home() {
  const push = usePush()
  // Switch tabs through the tab bar's navigator: `router.navigate('/activity')` would push another
  // list onto that tab's stack when it shows a detail.
  const navigation = useNavigation()
  const tabs = navigation.getParent()
  return (
    <HomeScreen
      receive={() => push('/receive')}
      send={() => push('/send')}
      showActivity={() => tabs?.navigate('activity')}
      showFederations={() => tabs?.navigate('federations')}
      showFederationDetails={() => push('/home/federation-detail')}
      showRecovery={() => push('/home/recovery')}
      showRecoveryPhrase={() => push('/recovery-phrase')}
      showOperation={(item) => openOperation(push, 'home', item)}
    />
  )
}
