import { Stack } from 'expo-router'
import {
  operationTitle,
  tabStackOptions,
} from '../../../../src/navigation/options'

export const unstable_settings = { initialRouteName: 'index' }

export default function HomeLayout() {
  return (
    <Stack screenOptions={tabStackOptions}>
      {/* Home's header row stands in for a navigation bar. Its title is only what a back button
          to it is called, which would otherwise read "index". */}
      <Stack.Screen
        name="index"
        options={{ headerShown: false, title: 'Back' }}
      />
      <Stack.Screen name="operation" options={operationTitle} />
      <Stack.Screen name="recovery" options={{ title: 'Recovery' }} />
      <Stack.Screen
        name="federation-detail"
        options={{ title: 'Federation' }}
      />
    </Stack>
  )
}
