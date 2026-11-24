import { Stack } from 'expo-router'
import {
  operationTitle,
  tabStackOptions,
} from '../../../../src/navigation/options'

export const unstable_settings = { initialRouteName: 'index' }

export default function HomeLayout() {
  return (
    // Home has no title for the back button to be labelled with, so it read "index".
    <Stack screenOptions={{ ...tabStackOptions, headerBackTitle: 'Back' }}>
      {/* Home's header row stands in for a navigation bar. */}
      <Stack.Screen name="index" options={{ headerShown: false }} />
      <Stack.Screen name="operation" options={operationTitle} />
      <Stack.Screen name="recovery" options={{ title: 'Recovery' }} />
      <Stack.Screen
        name="federation-detail"
        options={{ title: 'Federation' }}
      />
    </Stack>
  )
}
