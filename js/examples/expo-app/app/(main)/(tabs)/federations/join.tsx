import { useNavigation } from 'expo-router'
import { JoinScreen } from '../../../../src/screens/federations/JoinScreen'

export default function Join() {
  const navigation = useNavigation()
  return <JoinScreen close={navigation.goBack} />
}
