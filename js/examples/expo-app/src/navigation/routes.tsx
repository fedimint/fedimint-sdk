import { useCallback, useEffect } from 'react'
import { Text } from 'react-native'
import { useLocalSearchParams, useNavigation } from 'expo-router'
import { usePush } from './push'
import { OperationDetailScreen } from '../screens/activity/OperationDetailScreen'
import { FederationDetailScreen } from '../screens/federations/FederationDetailScreen'
import { RecoveryScreen } from '../screens/federations/RecoveryScreen'
import { holdOperation, openedOperation } from './options'
import { Screen } from '../components/Surfaces'
import { Palette, Typeface } from '../theme/tokens'

// Screens more than one tab pushes: an operation's detail, a federation's, and its recovery.

/** Back to the screen this one was pushed from, if there is one. */
function useClose() {
  const navigation = useNavigation()
  return useCallback(() => {
    if (navigation.canGoBack()) navigation.goBack()
  }, [navigation])
}

export function OperationRoute() {
  const { id } = useLocalSearchParams<{ id: string }>()
  const close = useClose()
  const item = openedOperation(id)
  useEffect(() => (id === undefined ? undefined : holdOperation(id)), [id])
  // Only a route restored without the list it came from, after a reload, has no item: say so,
  // rather than an empty screen under an empty title. Back, in the bar, still leaves.
  if (!item)
    return (
      <Screen>
        <Text style={[Typeface.callout, { color: Palette.muted }]}>
          This operation is no longer open. Go back and open it from the list.
        </Text>
      </Screen>
    )
  return <OperationDetailScreen item={item} close={close} />
}

export function RecoveryRoute() {
  return <RecoveryScreen close={useClose()} />
}

export function FederationDetailRoute({
  tab,
}: {
  tab: 'home' | 'federations'
}) {
  const push = usePush()
  return (
    <FederationDetailScreen showRecovery={() => push(`/${tab}/recovery`)} />
  )
}
