import { Stack, useRouter } from 'expo-router'
import { joinButton, tabStackOptions } from '../../../../src/navigation/options'

export const unstable_settings = { initialRouteName: 'index' }

export default function FederationsLayout() {
  const router = useRouter()
  return (
    <Stack screenOptions={tabStackOptions}>
      <Stack.Screen
        name="index"
        options={({ navigation }) => ({
          title: 'Federations',
          // Only from Federations in front, as `usePush`: a second tap would push a second Join.
          ...joinButton(() => {
            if (navigation.isFocused()) router.push('/federations/join')
          }),
        })}
      />
      <Stack.Screen name="join" options={{ title: 'Join' }} />
      <Stack.Screen
        name="federation-detail"
        options={{ title: 'Federation' }}
      />
      <Stack.Screen name="recovery" options={{ title: 'Recovery' }} />
    </Stack>
  )
}
