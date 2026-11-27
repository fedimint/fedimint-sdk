import { Stack } from 'expo-router'
import {
  operationTitle,
  tabStackOptions,
} from '../../../../src/navigation/options'

export const unstable_settings = { initialRouteName: 'index' }

export default function ActivityLayout() {
  return (
    <Stack screenOptions={tabStackOptions}>
      <Stack.Screen name="index" options={{ title: 'Activity' }} />
      <Stack.Screen name="operation" options={operationTitle} />
    </Stack>
  )
}
