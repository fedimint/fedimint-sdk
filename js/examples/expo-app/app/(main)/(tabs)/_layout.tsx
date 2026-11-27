import { Platform, PlatformColor } from 'react-native'
import { Tabs, withLayoutContext } from 'expo-router'
import {
  createNativeBottomTabNavigator,
  type NativeBottomTabNavigationEventMap,
  type NativeBottomTabNavigationOptions,
} from '@react-navigation/bottom-tabs/unstable'
import type {
  ParamListBase,
  TabNavigationState,
} from '@react-navigation/native'
import { Icon } from '../../../src/components/Icon'
import { Palette, Typeface } from '../../../src/theme/tokens'

export const unstable_settings = { initialRouteName: 'home' }

/** React Navigation's native tab bar, the one the React Native example uses, as a layout. */
const NativeTabs = withLayoutContext<
  NativeBottomTabNavigationOptions,
  ReturnType<typeof createNativeBottomTabNavigator>['Navigator'],
  TabNavigationState<ParamListBase>,
  NativeBottomTabNavigationEventMap
>(createNativeBottomTabNavigator().Navigator)

/** Each tab: its route, title, SF Symbol on iOS and icon on Android. */
const tabs = [
  { name: 'home', title: 'Home', symbol: 'house.fill', icon: 'house' },
  {
    name: 'activity',
    title: 'Activity',
    symbol: 'waveform.path.ecg',
    icon: 'waveform.path.ecg',
  },
  {
    name: 'federations',
    title: 'Federations',
    symbol: 'building.columns.fill',
    icon: 'building.columns',
  },
] as const

/** Options both tab bars share. */
const shared = { headerShown: false, tabBarActiveTintColor: Palette.primary }

/**
 * The design's three tabs. iOS uses the system tab bar: from iOS 26 that is the floating Liquid
 * Glass bar, which a tab bar drawn in JS does not match.
 */
export default function TabsLayout() {
  if (Platform.OS === 'ios') {
    return (
      <NativeTabs
        screenOptions={{
          ...shared,
          // UIKit's own unselected color: gray before iOS 26, the label color on the glass bar.
          ...(parseInt(String(Platform.Version), 10) < 26 && {
            tabBarInactiveTintColor: PlatformColor('systemGray'),
            // The screen's own ground with no hairline. This navigator gives UIKit one look for
            // both the bar's states, so it stays so with a list scrolled under it.
            tabBarStyle: {
              backgroundColor: Palette.background,
              shadowColor: 'transparent',
            },
            tabBarBlurEffect: 'none',
          }),
        }}
      >
        {tabs.map((tab) => (
          <NativeTabs.Screen
            key={tab.name}
            name={tab.name}
            options={{
              title: tab.title,
              tabBarIcon: { type: 'sfSymbol', name: tab.symbol },
            }}
          />
        ))}
      </NativeTabs>
    )
  }
  return (
    <Tabs
      screenOptions={{
        ...shared,
        tabBarInactiveTintColor: Palette.muted,
        tabBarLabelStyle: Typeface.geist('Medium', 11),
        tabBarStyle: { backgroundColor: Palette.surface },
      }}
    >
      {tabs.map((tab) => (
        <Tabs.Screen
          key={tab.name}
          name={tab.name}
          options={{
            title: tab.title,
            tabBarIcon: ({ color }) => (
              <Icon name={tab.icon} size={22} color={color} />
            ),
          }}
        />
      ))}
    </Tabs>
  )
}
